// Copyright 2026 the SoftPresenter contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SoftPresenter's addition to Vello 0.12: scratch buffers sized from what is drawn.
//!
//! The renderer's compute stages write lines, tiles, segments and per-tile command lists into
//! scratch buffers whose sizes must be fixed before the frame is sent to the GPU. Vello 0.12
//! gives each a constant size (about 165 MB in all, for every renderer) and never looks at
//! whether a frame outgrew them: a frame that did is silently not drawn.
//!
//! Here the sizes follow the drawing instead. Each frame's counters are read back before the
//! last stage runs; a frame that needed more room than it had is run again with more, and one
//! that fitted tells the next frame how much room to ask for.

use std::time::{Duration, Instant};

use crate::wgpu_engine::{ResourcePool, SIZE_CLASS_BITS};

use vello_encoding::{BufferSize, BumpAllocators, Layout, LineSoup, PathSegment, RenderConfig, SegmentCount, Tile};

/// How long the room a heavy frame needed is still asked for after it was last drawn.
const REMEMBERED: Duration = Duration::from_secs(3);

/// How long a spare buffer no frame has used is kept before it is given back to the GPU.
pub(crate) const KEPT_SPARE: Duration = Duration::from_secs(6);

/// How long a large spare buffer no frame has used is kept: long enough for the screens of
/// different sizes that one thread draws in turn to share it, and no longer.
pub(crate) const KEPT_LARGE_SPARE: Duration = Duration::from_secs(1);

/// No buffer a frame within the resting sizes uses is larger than this, on a target of up to
/// 4K: a larger one left spare was made for something heavier.
pub(crate) const LARGEST_RESTING_BUFFER: u64 = 16 << 20;

/// The most scratch memory one frame may be given, in bytes. Two of the heaviest readings in a
/// dissolve on a 4K screen take about half of it; a frame that needs more is refused, so that
/// a slide with a whole book pasted into it cannot take the computer's memory with it.
const MOST_BYTES: u64 = 1 << 30;

/// The room asked for, over what the heaviest remembered frame used: some spare, so that a
/// fade or a wipe that changes the picture from frame to frame rarely has to be run again.
const SPARE: (u64, u64) = (5, 4);

/// A tile's command list grows by this many words at a time (`PTCL_INCREMENT` in the shaders).
const PTCL_INCREMENT: u32 = 256;

/// Every tile starts with this many words of command list (`PTCL_INITIAL_ALLOC` in the
/// shaders).
const PTCL_INITIAL_ALLOC: u32 = 64;

/// The stages that can run out of room, as the shaders flag them (`shared/bump.wgsl`).
const STAGE_BINNING: u32 = 0x1;
const STAGE_TILE_ALLOC: u32 = 0x2;
const STAGE_FLATTEN: u32 = 0x4;
const STAGE_PATH_COUNT: u32 = 0x8;
const STAGE_COARSE: u32 = 0x10;

/// How many elements each scratch buffer holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScratchSizes {
    /// Flattened lines.
    pub lines: u32,
    /// Draw information and binning output, in words.
    pub bin_data: u32,
    /// Tiles.
    pub tiles: u32,
    /// Segment counts.
    pub seg_counts: u32,
    /// Path segments.
    pub segments: u32,
    /// Blend stack spill, in words.
    pub blend_spill: u32,
    /// Per-tile command lists, in words.
    pub ptcl: u32,
}

/// The bytes one element of each buffer takes.
const ELEMENT_BYTES: ScratchSizes = ScratchSizes {
    lines: size_of::<LineSoup>() as u32,
    bin_data: 4,
    tiles: size_of::<Tile>() as u32,
    seg_counts: size_of::<SegmentCount>() as u32,
    segments: size_of::<PathSegment>() as u32,
    blend_spill: 4,
    ptcl: 4,
};

impl ScratchSizes {
    /// The constant sizes Vello 0.12 gives every frame: about 165 MB.
    pub const UPSTREAM: Self = Self {
        lines: 1 << 21,
        bin_data: 1 << 18,
        tiles: 1 << 21,
        seg_counts: 1 << 21,
        segments: 1 << 21,
        blend_spill: 1 << 20,
        ptcl: 1 << 23,
    };

    /// The room kept ready when nothing heavy has been drawn lately: about 33 MB, on top of
    /// the command list every tile of the target starts with (8 MB for a 4K screen). A song
    /// slide of four lines in each of two languages, outlined and shadowed, makes about 95 000
    /// lines and 125 000 segments on a 1080p screen; this is room for four times that.
    pub const RESTING: Self = Self {
        lines: 1 << 19,
        bin_data: 1 << 14,
        tiles: 1 << 18,
        seg_counts: 1 << 19,
        segments: 1 << 19,
        blend_spill: 1 << 12,
        ptcl: 1 << 19,
    };

    /// A guess at the room a scene needs before any of it has been drawn, from how many path
    /// segments it has and how wide the target is, so that a heavy slide is rarely run more
    /// than once the first time it is shown. Measured on slides of outlined, shadowed text:
    /// each path segment makes about 3.5 lines and 3.5 tile segments on a thumbnail, and 5.5
    /// lines, 9 tile segments and 6.5 words of command list on a 4K screen. A wrong guess costs
    /// a run or some memory for one frame, never a wrong picture; and it stops at eight times
    /// the resting room, past which a scene is left to say what it needs.
    fn guessed(layout: &Layout, width_in_tiles: u32) -> Self {
        let tags = u64::from(layout.path_tags_size());
        // 240 tiles are 3840 pixels.
        let width = u64::from(width_in_tiles.min(240));
        let count = |tenths: u64| u32::try_from(tags * tenths / 10).unwrap_or(u32::MAX);
        let segments = count(35 + 60 * width / 240);
        let guess = Self {
            lines: count(40 + 22 * width / 240),
            bin_data: 0,
            tiles: count(10),
            seg_counts: segments,
            segments,
            blend_spill: 0,
            ptcl: count(10 + 60 * width / 240),
        };
        guess.at_most(Self::RESTING.zip(Self::RESTING, |n, _| n.saturating_mul(8)))
    }

    /// The bytes of GPU memory buffers of these sizes take.
    pub fn bytes(&self) -> u64 {
        let bytes = self.zip(ELEMENT_BYTES, |n, each| n.saturating_mul(each));
        [bytes.lines, bytes.bin_data, bytes.tiles, bytes.seg_counts, bytes.segments, bytes.blend_spill, bytes.ptcl]
            .into_iter()
            .map(u64::from)
            .sum()
    }

    fn zip(self, other: Self, f: impl Fn(u32, u32) -> u32) -> Self {
        Self {
            lines: f(self.lines, other.lines),
            bin_data: f(self.bin_data, other.bin_data),
            tiles: f(self.tiles, other.tiles),
            seg_counts: f(self.seg_counts, other.seg_counts),
            segments: f(self.segments, other.segments),
            blend_spill: f(self.blend_spill, other.blend_spill),
            ptcl: f(self.ptcl, other.ptcl),
        }
    }

    fn at_least(self, other: Self) -> Self {
        self.zip(other, u32::max)
    }

    fn at_most(self, other: Self) -> Self {
        self.zip(other, u32::min)
    }

    /// True when every buffer of these sizes holds what `needed` asks for.
    fn hold(&self, needed: &Self) -> bool {
        *self == self.at_least(*needed)
    }

    /// Each size with the spare added, rounded up to a size buffers are pooled at.
    fn with_spare(self) -> Self {
        self.zip(ELEMENT_BYTES, |n, each| {
            let spare = u64::from(n) * SPARE.0 / SPARE.1;
            pooled(u32::try_from(spare).unwrap_or(u32::MAX), each)
        })
    }

    /// The most elements each buffer can have when no buffer may be larger than `bytes`.
    fn within(bytes: u64) -> Self {
        // The engine rounds a buffer's size up to one it pools at, so the largest allowed is
        // the largest pooled size that fits.
        let mut largest = pool_class(bytes);
        while largest > bytes {
            // The next pooled size down is always more than three quarters of this one.
            largest = pool_class(largest / 4 * 3 + 1);
        }
        let count = |each: u32| u32::try_from(largest / u64::from(each)).unwrap_or(u32::MAX);
        Self::default().zip(ELEMENT_BYTES, |_, each| count(each))
    }
}

/// Rounds a size in bytes up to one the engine makes buffers at.
fn pool_class(bytes: u64) -> u64 {
    ResourcePool::size_class(bytes, SIZE_CLASS_BITS)
}

/// Rounds a count of elements up to all that fits in the buffer the engine would make for it:
/// asking for less wastes the difference.
fn pooled(count: u32, element_bytes: u32) -> u32 {
    let each = u64::from(element_bytes);
    u32::try_from(pool_class(u64::from(count.max(1)) * each) / each).unwrap_or(u32::MAX)
}

/// The scratch sizes one frame is given, and what the scene and target take of them anyway.
#[derive(Clone, Copy, Debug)]
pub struct ScratchPlan {
    /// The sizes this frame's buffers have.
    pub sizes: ScratchSizes,
    /// The least each buffer can be for this scene on this target, whatever is drawn.
    floor: ScratchSizes,
    /// Words of `bin_data` taken by draw information, before binning's own output.
    bin_data_start: u32,
    /// Words of command list the tiles start with, before any is added.
    ptcl_start: u32,
}

impl ScratchPlan {
    /// The plan for a scene of this layout on a target of this many tiles. The room it is
    /// given is counted from where the scene's and target's own share of `bin_data` and `ptcl`
    /// ends; no buffer is made larger than the room's limit unless that share alone is.
    pub(crate) fn new(layout: &Layout, width_in_tiles: u32, height_in_tiles: u32, room: ScratchRoom) -> Self {
        let ScratchRoom { wanted, limit, guess } = room;
        let wanted = if guess { wanted.at_least(ScratchSizes::guessed(layout, width_in_tiles)) } else { wanted };
        let bin_data_start = layout.bin_data_start;
        let ptcl_start = width_in_tiles.saturating_mul(height_in_tiles).saturating_mul(PTCL_INITIAL_ALLOC);
        // The shaders take a command list shorter than the tiles' own starts, or draw information
        // longer than its buffer, without flagging either: neither is ever allowed.
        let floor = ScratchSizes {
            lines: 1,
            bin_data: bin_data_start.saturating_add(1),
            tiles: 1,
            seg_counts: 1,
            segments: 1,
            blend_spill: 1,
            ptcl: ptcl_start.saturating_add(PTCL_INCREMENT),
        };
        let starts = ScratchSizes { bin_data: bin_data_start, ptcl: ptcl_start, ..ScratchSizes::default() };
        let sizes = wanted.zip(starts, u32::saturating_add).at_most(limit).at_least(floor);
        Self { sizes, floor, bin_data_start, ptcl_start }
    }

    /// Writes the plan's sizes into a frame's configuration, in place of Vello's constants.
    pub(crate) fn apply(&self, config: &mut RenderConfig) {
        let sizes = &self.sizes;
        config.buffer_sizes.lines = BufferSize::new(sizes.lines);
        config.buffer_sizes.bin_data = BufferSize::new(sizes.bin_data);
        config.buffer_sizes.tiles = BufferSize::new(sizes.tiles);
        config.buffer_sizes.seg_counts = BufferSize::new(sizes.seg_counts);
        config.buffer_sizes.segments = BufferSize::new(sizes.segments);
        config.buffer_sizes.blend_spill = BufferSize::new(sizes.blend_spill);
        config.buffer_sizes.ptcl = BufferSize::new(sizes.ptcl);
        config.gpu.lines_size = sizes.lines;
        config.gpu.binning_size = sizes.bin_data - self.bin_data_start;
        config.gpu.tiles_size = sizes.tiles;
        config.gpu.seg_counts_size = sizes.seg_counts;
        config.gpu.segments_size = sizes.segments;
        config.gpu.blend_size = sizes.blend_spill;
        config.gpu.ptcl_size = sizes.ptcl;
    }

    /// What the frame used of each buffer, by the counters the GPU kept. Counters of stages
    /// after one that ran out of room are too low: those stages did not run.
    fn used(&self, bump: &BumpAllocators) -> ScratchSizes {
        ScratchSizes {
            lines: bump.lines,
            bin_data: self.bin_data_start.saturating_add(bump.binning),
            tiles: bump.tile,
            seg_counts: bump.seg_counts,
            segments: bump.segments,
            blend_spill: bump.blend,
            ptcl: self.ptcl_start.saturating_add(bump.ptcl),
        }
    }

    /// Sizes without the scene's and target's own share of the two shared buffers, so that
    /// they can be carried from one frame to another of a different size.
    fn beyond_starts(&self, sizes: ScratchSizes) -> ScratchSizes {
        ScratchSizes {
            bin_data: sizes.bin_data.saturating_sub(self.bin_data_start),
            ptcl: sizes.ptcl.saturating_sub(self.ptcl_start),
            ..sizes
        }
    }

    /// The sizes to run the frame again with, after it ran out of room. The stage that ran out
    /// first gets what it asked for and some spare; the stages after it, which did not run and
    /// so could not say, get twice what they had.
    fn grown(&self, bump: &BumpAllocators, limit: ScratchSizes) -> ScratchSizes {
        let used = self.used(bump);
        let had = self.sizes;
        // The stages in the order they run, each with the buffers it fills.
        let short = [
            bump.failed & STAGE_FLATTEN != 0 || used.lines > had.lines,
            bump.failed & STAGE_BINNING != 0 || used.bin_data > had.bin_data,
            bump.failed & STAGE_TILE_ALLOC != 0 || used.tiles > had.tiles,
            bump.failed & STAGE_PATH_COUNT != 0 || used.seg_counts > had.seg_counts,
            bump.failed & STAGE_COARSE != 0
                || used.ptcl > had.ptcl
                || used.blend_spill > had.blend_spill
                || used.segments > had.segments,
        ];
        let first = short.iter().position(|short| *short).unwrap_or(0);
        let stage =
            ScratchSizes { lines: 0, bin_data: 1, tiles: 2, seg_counts: 3, segments: 4, blend_spill: 4, ptcl: 4 };
        let asked = used.with_spare().at_least(had);
        let doubled = had.zip(ELEMENT_BYTES, |n, each| pooled(n.saturating_mul(2), each));
        let later = stage.zip(doubled, |stage, doubled| if stage as usize > first { doubled } else { 0 });
        let guessed = asked.at_least(later).at_most(limit).at_least(self.floor);
        if guessed.bytes() <= MOST_BYTES {
            return guessed;
        }
        // Doubling the later stages by guess would pass what one frame may have: give only
        // what is known to be needed, and let the later stages ask in their turn.
        asked.at_most(limit).at_least(self.floor)
    }
}

/// The scratch room to give a frame, and the most the GPU lets any frame have.
#[derive(Clone, Copy, Debug)]
pub struct ScratchRoom {
    /// The room to give the frame, counted from where the scene's and target's own share of
    /// the two shared buffers ends.
    pub wanted: ScratchSizes,
    /// The most elements each buffer may have.
    pub limit: ScratchSizes,
    /// Whether to give the frame more than `wanted` when the scene looks as if it needs it.
    pub guess: bool,
}

/// What a frame's counters say about the room it was given.
#[derive(Debug)]
pub(crate) enum Verdict {
    /// Everything fitted: the last stage can run.
    Fits,
    /// Something ran out of room: run the frame again with this much.
    Again(ScratchSizes),
    /// The frame needs more room than the renderer may use.
    TooHeavy,
}

/// A renderer's memory of how much scratch room its recent frames used.
#[derive(Debug)]
pub(crate) struct Scratch {
    /// Sizes used for every frame as they are, never grown: Vello's own behaviour.
    fixed: Option<ScratchSizes>,
    /// The most any frame used since `since`, and in the stretch of time before that; both
    /// without the scene's and target's own share.
    lately: ScratchSizes,
    before: ScratchSizes,
    since: Instant,
    /// The most elements the GPU lets each buffer have.
    limit: ScratchSizes,
    /// How many times a frame had to be run again for want of room.
    reruns: u64,
    /// The scenes drawn lately that fitted, newest last: each one's fingerprint and what it
    /// used of every buffer.
    fitted: Vec<(u64, ScratchSizes)>,
    /// How many frames were drawn without waiting for their counters.
    unwaited: u64,
}

/// How many scenes that fitted are kept in mind: a few screens of different sizes, each
/// showing one scene or fading between two.
const FITTED_KEPT: usize = 8;

impl Scratch {
    /// A new memory, for a GPU whose largest buffer a stage can use is this many bytes.
    pub(crate) fn new(largest_buffer: u64) -> Self {
        Self {
            fixed: None,
            lately: ScratchSizes::default(),
            before: ScratchSizes::default(),
            since: Instant::now(),
            limit: ScratchSizes::within(largest_buffer),
            reruns: 0,
            fitted: Vec::new(),
            unwaited: 0,
        }
    }

    pub(crate) fn set_fixed(&mut self, sizes: Option<ScratchSizes>) {
        self.fixed = sizes;
    }

    pub(crate) fn reruns(&self) -> u64 {
        self.reruns
    }

    pub(crate) fn unwaited(&self) -> u64 {
        self.unwaited
    }

    /// True when a scene with this fingerprint was drawn before and fitted in no more room
    /// than this plan gives. The GPU's stages count the same for the same scene and target
    /// size every time, so it fits again and its counters need not be read.
    pub(crate) fn fitted_before(&mut self, print: u64, plan: &ScratchPlan, now: Instant) -> bool {
        let Some(at) = self.fitted.iter().position(|(known, _)| *known == print) else { return false };
        let (_, used) = self.fitted[at];
        if !plan.sizes.hold(&used) {
            return false;
        }
        // Newest last, and the room it uses stays remembered while it keeps being drawn.
        let known = self.fitted.remove(at);
        self.fitted.push(known);
        self.remember(plan.beyond_starts(used), now);
        self.unwaited += 1;
        true
    }

    #[cfg(test)]
    fn limit(&self) -> ScratchSizes {
        self.limit
    }

    /// Room for one frame: this much, within what the GPU allows.
    pub(crate) fn room(&self, wanted: ScratchSizes) -> ScratchRoom {
        ScratchRoom { wanted, limit: self.limit, guess: self.fixed.is_none() }
    }

    /// The room to give a frame drawn now, counted beyond the shared buffers' starts.
    pub(crate) fn wanted(&self, now: Instant) -> ScratchSizes {
        match self.fixed {
            Some(sizes) => sizes,
            None => self.remembered(now).with_spare().at_least(ScratchSizes::RESTING).at_most(self.limit),
        }
    }

    /// The most a frame used in the time still remembered now.
    fn remembered(&self, now: Instant) -> ScratchSizes {
        let age = now.saturating_duration_since(self.since);
        if age >= REMEMBERED * 2 {
            ScratchSizes::default()
        } else if age >= REMEMBERED {
            self.lately
        } else {
            self.lately.at_least(self.before)
        }
    }

    /// True when a target of this size can be drawn at all: its tiles' own command lists fit
    /// in one buffer.
    pub(crate) fn allows(&self, width: u32, height: u32) -> bool {
        let nothing = ScratchRoom { wanted: ScratchSizes::default(), limit: self.limit, guess: false };
        let plan = ScratchPlan::new(&Layout::default(), width.div_ceil(16), height.div_ceil(16), nothing);
        self.limit.hold(&plan.sizes)
    }

    /// True while a frame heavier than the resting sizes allow for is remembered.
    pub(crate) fn is_heavy(&self, now: Instant) -> bool {
        self.fixed.is_none() && self.wanted(now) != ScratchSizes::RESTING.at_most(self.limit)
    }

    /// Judges a frame by the counters read back from the GPU.
    pub(crate) fn judge(&mut self, plan: &ScratchPlan, bump: &BumpAllocators, print: u64, now: Instant) -> Verdict {
        let used = plan.used(bump);
        if bump.failed == 0 && plan.sizes.hold(&used) {
            self.remember(plan.beyond_starts(used), now);
            self.fitted.retain(|(known, _)| *known != print);
            if self.fitted.len() >= FITTED_KEPT {
                self.fitted.remove(0);
            }
            self.fitted.push((print, used));
            return Verdict::Fits;
        }
        let grown = plan.grown(bump, self.limit);
        // Refused when told to keep to fixed sizes, when what is known to be needed is more
        // than the GPU allows, or when nothing is left to grow.
        if self.fixed.is_some() || !self.limit.hold(&used) || grown == plan.sizes || grown.bytes() > MOST_BYTES {
            return Verdict::TooHeavy;
        }
        self.reruns += 1;
        Verdict::Again(plan.beyond_starts(grown))
    }

    fn remember(&mut self, used: ScratchSizes, now: Instant) {
        if now.saturating_duration_since(self.since) >= REMEMBERED {
            // What was remembered moves back one stretch of time, or out of memory if no frame
            // was drawn for two.
            self.before = self.remembered(now);
            self.lately = ScratchSizes::default();
            self.since = now;
        }
        self.lately = self.lately.at_least(used);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_LIMIT: u64 = 1 << 40;
    const PRINT: u64 = 7;

    fn plan_within(wanted: ScratchSizes, scratch: &Scratch) -> ScratchPlan {
        // A 3840 by 2160 target is 240 by 135 tiles.
        let layout = Layout { bin_data_start: 1000, ..Layout::default() };
        ScratchPlan::new(&layout, 240, 135, ScratchRoom { wanted, limit: scratch.limit(), guess: false })
    }

    #[test]
    fn sizes_are_rounded_up_to_what_a_pooled_buffer_holds() {
        assert_eq!(pool_class(9), 10);
        assert_eq!(pool_class(125_829_120), 134_217_728);
        assert_eq!(pool_class(100_663_296), 100_663_296);
        assert_eq!(pooled(1 << 20, 4), 1 << 20);
        assert_eq!(pooled((1 << 20) + 1, 4), 5 << 18);
        // 24-byte elements: a million of them go in a 24 MiB buffer, which holds 1 048 576.
        assert_eq!(pooled(1_000_000, 24), 1 << 20);
        assert_eq!(pooled(u32::MAX, 24), u32::MAX);
    }

    #[test]
    fn no_buffer_is_asked_to_be_larger_than_the_gpu_allows() {
        // 128 MiB, the usual limit: the engine would round 120 MiB up to 128, and 130 up to 160.
        let limit = ScratchSizes::within(128 << 20);
        assert_eq!(limit.lines, (128 << 20) / 24);
        assert_eq!(u64::from(limit.ptcl) * 4, 128 << 20);
        assert_eq!(u64::from(ScratchSizes::within(100 << 20).ptcl) * 4, 96 << 20);
        assert_eq!(u64::from(ScratchSizes::within(127 << 20).ptcl) * 4, 112 << 20);
        let scratch = Scratch::new(128 << 20);
        assert!(scratch.allows(3840, 2160));
        assert!(!Scratch::new(4 << 20).allows(3840, 2160), "a 4K target's command lists alone are 8 MB");
    }

    #[test]
    fn a_scene_of_many_path_segments_is_given_more_room_before_it_is_drawn() {
        let scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        // A song slide: about twenty thousand path segments. The resting room is enough.
        let song = Layout { path_tag_base: 0, path_data_base: 20_480 / 4, ..Layout::default() };
        assert_eq!(song.path_tags_size(), 20_480);
        let plan = ScratchPlan::new(&song, 240, 135, scratch.room(scratch.wanted(start)));
        assert_eq!(plan.sizes.lines, ScratchSizes::RESTING.lines);
        // A full reading: 480 000. On a 4K screen it was measured to make 2.6 million lines and
        // 3.7 million tile segments; as a thumbnail, 1.7 million of each.
        let reading = Layout { path_tag_base: 0, path_data_base: 479_232 / 4, ..Layout::default() };
        let plan = ScratchPlan::new(&reading, 240, 135, scratch.room(scratch.wanted(start)));
        assert!(plan.sizes.lines > 2_600_000 && plan.sizes.segments > 3_700_000, "{:?}", plan.sizes);
        assert!(plan.sizes.segments <= ScratchSizes::RESTING.segments * 8, "the guess has an end");
        let thumb = ScratchPlan::new(&reading, 30, 17, scratch.room(scratch.wanted(start)));
        assert!(thumb.sizes.lines > 1_750_000 && thumb.sizes.lines < plan.sizes.lines, "{:?}", thumb.sizes);
        // Sizes told to stay fixed are not guessed at.
        let mut fixed = Scratch::new(NO_LIMIT);
        fixed.set_fixed(Some(ScratchSizes::RESTING));
        let plan = ScratchPlan::new(&reading, 240, 135, fixed.room(fixed.wanted(start)));
        assert_eq!(plan.sizes.lines, ScratchSizes::RESTING.lines);
    }

    #[test]
    fn a_plan_never_goes_under_what_the_target_and_scene_take_anyway() {
        let scratch = Scratch::new(NO_LIMIT);
        let plan = plan_within(ScratchSizes::default(), &scratch);
        assert_eq!(plan.sizes.ptcl, 240 * 135 * 64 + 256);
        assert_eq!(plan.sizes.bin_data, 1001);
        assert!(plan.sizes.lines >= 1 && plan.sizes.tiles >= 1 && plan.sizes.blend_spill >= 1);
    }

    #[test]
    fn a_frame_that_fits_is_remembered_and_then_forgotten() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        let plan = plan_within(scratch.wanted(start), &scratch);
        assert!(!scratch.is_heavy(start));
        let heavy = ScratchSizes::RESTING.lines - 1;
        let bump = BumpAllocators { lines: heavy, ..BumpAllocators::default() };
        assert!(matches!(scratch.judge(&plan, &bump, PRINT, start), Verdict::Fits));
        assert!(scratch.wanted(start).lines >= heavy / 4 * 5, "the next frame gets the room and some spare");
        assert!(scratch.is_heavy(start));
        // Light frames a while later: the heavy one is kept for one stretch, then dropped.
        let light = BumpAllocators::default();
        assert!(matches!(scratch.judge(&plan, &light, PRINT, start + REMEMBERED), Verdict::Fits));
        assert!(scratch.wanted(start + REMEMBERED).lines > ScratchSizes::RESTING.lines);
        assert!(matches!(scratch.judge(&plan, &light, PRINT, start + REMEMBERED * 2), Verdict::Fits));
        assert_eq!(scratch.wanted(start + REMEMBERED * 2), ScratchSizes::RESTING);
    }

    #[test]
    fn a_heavy_frame_is_forgotten_by_the_clock_alone_when_nothing_more_is_drawn() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        let plan = plan_within(scratch.wanted(start), &scratch);
        let bump = BumpAllocators { lines: ScratchSizes::RESTING.lines - 1, ..BumpAllocators::default() };
        assert!(matches!(scratch.judge(&plan, &bump, PRINT, start), Verdict::Fits));
        assert!(scratch.is_heavy(start + REMEMBERED));
        assert!(!scratch.is_heavy(start + REMEMBERED * 2));
        assert_eq!(scratch.wanted(start + REMEMBERED * 2), ScratchSizes::RESTING);
    }

    #[test]
    fn a_frame_that_ran_out_is_run_again_with_room_for_what_it_asked() {
        let start = Instant::now();
        let mut scratch = Scratch::new(NO_LIMIT);
        let plan = plan_within(scratch.wanted(start), &scratch);
        // Flattening made five times the lines there was room for; nothing after it ran.
        let asked = plan.sizes.lines * 5;
        let bump = BumpAllocators { failed: STAGE_FLATTEN, lines: asked, ..BumpAllocators::default() };
        let Verdict::Again(more) = scratch.judge(&plan, &bump, PRINT, Instant::now()) else {
            panic!("should run again")
        };
        let again = plan_within(more, &scratch);
        assert!(again.sizes.lines >= asked);
        assert!(again.sizes.tiles >= plan.sizes.tiles * 2, "later stages could not say, so they double");
        assert!(again.sizes.ptcl >= plan.sizes.ptcl * 2);
        assert_eq!(scratch.reruns(), 1);
    }

    #[test]
    fn segments_past_their_buffer_are_caught_though_no_stage_flags_them() {
        let start = Instant::now();
        let mut scratch = Scratch::new(NO_LIMIT);
        let plan = plan_within(scratch.wanted(start), &scratch);
        let bump = BumpAllocators { segments: plan.sizes.segments + 1, ..BumpAllocators::default() };
        let Verdict::Again(more) = scratch.judge(&plan, &bump, PRINT, Instant::now()) else {
            panic!("should run again")
        };
        assert!(more.segments > plan.sizes.segments);
        assert_eq!(more.lines, plan.sizes.lines, "earlier stages fitted and keep their sizes");
    }

    #[test]
    fn a_frame_too_heavy_for_the_gpu_or_for_fixed_sizes_is_refused() {
        let start = Instant::now();
        let mut scratch = Scratch::new(64 << 20);
        let plan = plan_within(scratch.wanted(start), &scratch);
        let bump = BumpAllocators { failed: STAGE_FLATTEN, lines: 1 << 24, ..BumpAllocators::default() };
        assert!(matches!(scratch.judge(&plan, &bump, PRINT, Instant::now()), Verdict::TooHeavy));
        // Needing a little more than the spare allows, but no more than the GPU does, is fine.
        let most = scratch.limit().lines;
        let bump = BumpAllocators { failed: STAGE_FLATTEN, lines: most - 1, ..BumpAllocators::default() };
        let Verdict::Again(more) = scratch.judge(&plan, &bump, PRINT, Instant::now()) else {
            panic!("should run again")
        };
        assert_eq!(more.lines, most);

        let mut fixed = Scratch::new(NO_LIMIT);
        fixed.set_fixed(Some(ScratchSizes::RESTING));
        let plan = plan_within(fixed.wanted(start), &fixed);
        let bump = BumpAllocators { failed: STAGE_FLATTEN, lines: 1 << 20, ..BumpAllocators::default() };
        assert!(matches!(fixed.judge(&plan, &bump, PRINT, Instant::now()), Verdict::TooHeavy));
    }

    #[test]
    fn a_scene_that_fitted_before_is_not_waited_for_again() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        let plan = plan_within(scratch.wanted(start), &scratch);
        assert!(!scratch.fitted_before(PRINT, &plan, start), "never seen");
        let bump = BumpAllocators { lines: 5_000, tile: 700, ..BumpAllocators::default() };
        assert!(matches!(scratch.judge(&plan, &bump, PRINT, start), Verdict::Fits));
        assert!(scratch.fitted_before(PRINT, &plan, start));
        assert!(!scratch.fitted_before(PRINT + 1, &plan, start), "another scene is still checked");
        assert_eq!(scratch.unwaited(), 1);
        // Given less room than it used, it is checked again.
        let tight = ScratchSizes { lines: 4_000, ..ScratchSizes::RESTING };
        assert!(!scratch.fitted_before(PRINT, &plan_within(tight, &scratch), start));
        // Only the newest few scenes are kept in mind.
        for other in 0..FITTED_KEPT as u64 {
            assert!(matches!(scratch.judge(&plan, &bump, 100 + other, start), Verdict::Fits));
        }
        assert!(!scratch.fitted_before(PRINT, &plan, start));
    }

    #[test]
    fn a_frame_is_never_given_more_than_the_most_one_frame_may_have() {
        let mut scratch = Scratch::new(NO_LIMIT);
        let start = Instant::now();
        // Room close to the most a frame may have, and a few more lines than fit.
        let had =
            ScratchSizes { lines: 8_000_000, seg_counts: 20_000_000, segments: 20_000_000, ..ScratchSizes::RESTING };
        let plan = plan_within(had, &scratch);
        assert!(plan.sizes.bytes() < MOST_BYTES);
        let bump = BumpAllocators { failed: STAGE_FLATTEN, lines: 9_000_000, ..BumpAllocators::default() };
        // Doubling the stages after flattening would pass the most: only the lines grow.
        let Verdict::Again(more) = scratch.judge(&plan, &bump, PRINT, start) else { panic!("should run again") };
        assert!(more.lines >= 9_000_000);
        assert_eq!(more.segments, had.segments, "the later stages are not doubled past the most");
        assert!(plan_within(more, &scratch).sizes.bytes() <= MOST_BYTES);
        // Lines alone past the most: refused.
        let bump = BumpAllocators { failed: STAGE_FLATTEN, lines: 50_000_000, ..BumpAllocators::default() };
        assert!(matches!(scratch.judge(&plan, &bump, PRINT, start), Verdict::TooHeavy));
    }
}
