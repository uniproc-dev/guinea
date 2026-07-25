use windows_canvas::ColorF;

/// Builds an opaque color from a packed `0xRRGGBB` literal.
pub const fn hex(rgb: u32) -> ColorF {
    ColorF::from_rgb8(((rgb >> 16) & 0xFF) as u8, ((rgb >> 8) & 0xFF) as u8, (rgb & 0xFF) as u8)
}

/// Builds a color from a packed `0xRRGGBB` literal and an 8-bit alpha.
pub const fn hex_alpha(rgb: u32, a: u8) -> ColorF {
    ColorF::from_rgba8(((rgb >> 16) & 0xFF) as u8, ((rgb >> 8) & 0xFF) as u8, (rgb & 0xFF) as u8, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_splits_packed_rgb_into_normalized_components() {
        let c = hex(0xff8000);
        assert_eq!(c.r, 1.0);
        assert!((c.g - (0x80 as f32 / 255.0)).abs() < f32::EPSILON);
        assert_eq!(c.b, 0.0);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn hex_alpha_applies_the_given_alpha_byte() {
        let c = hex_alpha(0xffffff, 36);
        assert_eq!(c.r, 1.0);
        assert!((c.a - (36.0 / 255.0)).abs() < f32::EPSILON);
    }
}
