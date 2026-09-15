//! Client-area pixel analysis for the announcement overlay: frame storage,
//! hand-rolled HSV masking and frame diffing. No image crate is used.

/// One top-down BGRA frame of the terminal client area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
}

impl Frame {
    /// Fixture helper: production frames only come from [`Frame::from_pixels`].
    #[cfg(test)]
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            bgra: vec![0; width.saturating_mul(height) * 4],
        }
    }

    pub fn from_pixels(width: usize, height: usize, bgra: Vec<u8>) -> Option<Self> {
        (width.saturating_mul(height) * 4 == bgra.len()).then_some(Self {
            width,
            height,
            bgra,
        })
    }

    pub fn len(&self) -> usize {
        self.width * self.height
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn rgb(&self, x: usize, y: usize) -> (u8, u8, u8) {
        let offset = (y * self.width + x) * 4;
        (
            self.bgra[offset + 2],
            self.bgra[offset + 1],
            self.bgra[offset],
        )
    }

    #[cfg(test)]
    pub fn set_rgb(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        let offset = (y * self.width + x) * 4;
        self.bgra[offset] = b;
        self.bgra[offset + 1] = g;
        self.bgra[offset + 2] = r;
        self.bgra[offset + 3] = 255;
    }

    #[cfg(test)]
    pub fn fill(&mut self, r: u8, g: u8, b: u8) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.set_rgb(x, y, r, g, b);
            }
        }
    }

    /// True when every pixel is identical, i.e. the render produced nothing.
    pub fn is_uniform(&self) -> bool {
        let Some(first) = self.bgra.get(..4) else {
            return true;
        };
        self.bgra.chunks_exact(4).all(|pixel| pixel == first)
    }
}

/// Axis-aligned bounds of one masked region; `right`/`bottom` are inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blob {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub area: usize,
}

impl Blob {
    pub fn width(self) -> i32 {
        self.right - self.left + 1
    }

    pub fn height(self) -> i32 {
        self.bottom - self.top + 1
    }

    pub fn center(self) -> (i32, i32) {
        (
            (self.left + self.right + 1) / 2,
            (self.top + self.bottom + 1) / 2,
        )
    }

    /// Share of the bounding box actually covered by the mask.
    pub fn fill_ratio(self) -> f64 {
        let box_area = (self.width().max(1) * self.height().max(1)) as f64;
        self.area as f64 / box_area
    }
}

/// OpenCV-style HSV: hue in degrees `0..360`, saturation and value in `0..=1`.
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (rf, gf, bf) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == rf {
        60.0 * ((gf - bf) / delta).rem_euclid(6.0)
    } else if max == gf {
        60.0 * ((bf - rf) / delta + 2.0)
    } else {
        60.0 * ((rf - gf) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue.rem_euclid(360.0), saturation, max)
}

/// Mask predicate for one HSV band; the band must not wrap around `0`.
fn in_band(pixel: (u8, u8, u8), lo: (f64, f64, f64), hi: (f64, f64, f64)) -> bool {
    let (hue, saturation, value) = rgb_to_hsv(pixel.0, pixel.1, pixel.2);
    hue >= lo.0
        && hue <= hi.0
        && saturation >= lo.1
        && saturation <= hi.1
        && value >= lo.2
        && value <= hi.2
}

/// Every 4-connected region of pixels inside the HSV band, in scan order.
pub fn blobs(frame: &Frame, lo: (f64, f64, f64), hi: (f64, f64, f64)) -> Vec<Blob> {
    let mut visited = vec![false; frame.len()];
    let mut found = Vec::new();
    let mut stack = Vec::new();
    for seed in 0..frame.len() {
        if visited[seed] {
            continue;
        }
        visited[seed] = true;
        let (seed_x, seed_y) = (seed % frame.width, seed / frame.width);
        if !in_band(frame.rgb(seed_x, seed_y), lo, hi) {
            continue;
        }
        stack.push(seed);
        let mut blob = Blob {
            left: seed_x as i32,
            top: seed_y as i32,
            right: seed_x as i32,
            bottom: seed_y as i32,
            area: 0,
        };
        while let Some(index) = stack.pop() {
            let (x, y) = (index % frame.width, index / frame.width);
            blob.area += 1;
            blob.left = blob.left.min(x as i32);
            blob.top = blob.top.min(y as i32);
            blob.right = blob.right.max(x as i32);
            blob.bottom = blob.bottom.max(y as i32);
            // 4-neighbourhood; a wrapping borrow lands outside the frame.
            for (next_x, next_y) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if next_x >= frame.width || next_y >= frame.height {
                    continue;
                }
                let next = next_y * frame.width + next_x;
                if visited[next] || !in_band(frame.rgb(next_x, next_y), lo, hi) {
                    continue;
                }
                visited[next] = true;
                stack.push(next);
            }
        }
        found.push(blob);
    }
    found
}

/// Channel tolerance that still counts as "the same pixel" (repaint noise).
pub const PIXEL_TOLERANCE: u8 = 8;

/// Share of pixels that differ between two same-sized frames.
pub fn diff_ratio(left: &Frame, right: &Frame) -> Option<f64> {
    if left.width != right.width || left.height != right.height || left.is_empty() {
        return None;
    }
    let changed = left
        .bgra
        .chunks_exact(4)
        .zip(right.bgra.chunks_exact(4))
        .filter(|(a, b)| (0..3).any(|channel| a[channel].abs_diff(b[channel]) > PIXEL_TOLERANCE))
        .count();
    Some(changed as f64 / left.len() as f64)
}
