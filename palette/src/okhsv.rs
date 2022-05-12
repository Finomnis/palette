use crate::white_point::D65;
use approx::AbsDiffEq;
use core::fmt::Debug;

use crate::convert::IntoColorUnclamped;
use crate::num::{FromScalar, Hypot, Powi, Recip, Sqrt};
use crate::ok_utils::{LC, ST};
use crate::{
    angle::RealAngle,
    convert::FromColorUnclamped,
    num::{Arithmetics, Cbrt, IsValidDivisor, MinMax, One, Real, Trigonometry, Zero},
    ok_utils, Alpha, HasBoolMask, LinSrgb, Okhwb, Oklab, OklabHue,
};

/// Okhsv with an alpha component. See the [`Okhsva` implementation in
/// `Alpha`](crate::Alpha#Okhsva).
pub type Okhsva<T = f32> = Alpha<Okhsv<T>, T>;

/// A Hue/Saturation/Value representation of [`Oklab`].
///
/// Allows
/// * changing lightness/chroma/saturation while keeping perceived Hue constant
/// (like HSV promises but delivers only partially)  
/// * finding the strongest color (maximum chroma) at s == 1 (like HSV)  
#[derive(Debug, ArrayCast, FromColorUnclamped, WithAlpha)]
#[cfg_attr(feature = "serializing", derive(Serialize, Deserialize))]
#[palette(
    palette_internal,
    white_point = "D65",
    component = "T",
    skip_derives(Oklab, Okhwb)
)]
#[repr(C)]
pub struct Okhsv<T = f32> {
    /// The hue of the color, in degrees of a circle, where for all `h`: `h+n*360 ==  h`.
    ///
    /// For fully saturated, bright colors
    /// * 0° corresponds to a kind of magenta-pink (RBG #ff0188),
    /// * 90° to a kind of yellow (RBG RGB #ffcb00)
    /// * 180° to a kind of cyan (RBG #00ffe1) and
    /// * 240° to a kind of blue (RBG #00aefe).
    ///
    /// For s == 0 or v == 0, the hue is irrelevant.
    #[palette(unsafe_same_layout_as = "T")]
    pub hue: OklabHue<T>,

    /// The saturation (freedom of whitishness) of the color.
    ///
    /// * `0.0` corresponds to pure mixture of black and white without any color.
    /// The black to white relation depends on v.
    /// * `1.0` to a fully saturated color without any white.
    ///
    /// For v == 0 the saturation is irrelevant.
    pub saturation: T,

    /// The monochromatic brightness of the color.
    /// * `0.0` corresponds to pure black
    /// * `1.0` corresponds to a maximally bright colour -- be it very colorful or very  white
    pub value: T,
}

impl<T> Copy for Okhsv<T> where T: Copy {}

impl<T> Clone for Okhsv<T>
where
    T: Clone,
{
    fn clone(&self) -> Okhsv<T> {
        Okhsv {
            hue: self.hue.clone(),
            saturation: self.saturation.clone(),
            value: self.value.clone(),
        }
    }
}

impl<T> Okhsv<T> {
    /// Create an Okhsv color.
    pub fn new<H: Into<OklabHue<T>>>(hue: H, saturation: T, value: T) -> Self {
        Self {
            hue: hue.into(),
            saturation,
            value,
        }
    }

    /// Convert to a `(h, s, v)` tuple.
    pub fn into_components(self) -> (OklabHue<T>, T, T) {
        (self.hue, self.saturation, self.value)
    }

    /// Convert from a `(h, s, v)` tuple.
    pub fn from_components<H: Into<OklabHue<T>>>((hue, saturation, value): (H, T, T)) -> Self {
        Self::new(hue, saturation, value)
    }
}

/// Converts `lab` to `Okhsv` in the bounds of sRGB.
///
/// # See
/// See [`srgb_to_okhsv`](https://bottosson.github.io/posts/colorpicker/#hsv-2).
/// This implementation differs from srgb_to_okhsv in that it starts with the `lab`
/// value and produces hues in degrees, whereas `srgb_to_okhsv` produces degree/360.
impl<T> FromColorUnclamped<Oklab<T>> for Okhsv<T>
where
    T: Real
        + AbsDiffEq
        + PartialOrd
        + HasBoolMask<Mask = bool>
        + MinMax
        + Copy
        + Powi
        + Sqrt
        + Cbrt
        + Arithmetics
        + Trigonometry
        + Zero
        + Hypot
        + One
        + FromScalar
        + Debug
        + RealAngle,
    T::Scalar: Real
        + Zero
        + One
        + Recip
        + Hypot
        + IsValidDivisor<Mask = bool>
        + Arithmetics
        + Clone
        + FromScalar<Scalar = T::Scalar>
        + Debug,
{
    fn from_color_unclamped(lab: Oklab<T>) -> Self {
        if lab.a == T::zero() && lab.b == T::zero() {
            // `a` describes how green/red the color is, `b` how blue/yellow the color is
            // both are zero -> the color is totally desaturated.
            let v = ok_utils::toe(lab.l);

            return Self::new(T::zero(), T::zero(), v);
        }

        // compute hue and chroma as for OkLCh
        // we will use h as is.
        let mut C = T::hypot(lab.a, lab.b);
        let a_ = lab.a / C;
        let b_ = lab.b / C;

        let mut L = lab.l;
        // use negative a and be and rotate, to ensure hue is normalized
        let h = T::from_f64(180.0) + T::atan2(-lab.b, -lab.a).radians_to_degrees();

        // For each hue the sRGB gamut can be drawn on a 2-dimensional space.
        // Let L_r, the lightness in relation to the possible luminance of sRGB, be spread
        // along the y-axis (bottom is black, top is bright) and Chroma along the x-axis
        // (left is desaturated, right is colorful). The gamut then takes a triangular shape,
        // with a concave top side and a cusp to the right.
        // To use saturation and brightness values, the gamut must be mapped to a square.
        // The lower point of the triangle is expanded to the lower side of the square.
        // The left side remains unchanged and the cusp of the triangle moves to the upper right.
        let cusp = LC::find_cusp(a_, b_);
        let ST_max: ST<T> = cusp.into();
        let S_0 = T::from_f64(0.5);
        let k = T::one() - S_0 / ST_max.s;

        // first we find L_v, C_v, L_vt and C_vt
        let t = ST_max.t / (C + L * ST_max.t);
        let L_v = t * L;
        let C_v = t * C;

        let L_vt = ok_utils::toe_inv(L_v);
        let C_vt = C_v * L_vt / L_v;

        // we can then use these to invert the step that compensates for the toe and the curved top part of the triangle:
        let rgb_scale: LinSrgb<T> = Oklab::new(L_vt, a_ * C_vt, b_ * C_vt).into_color_unclamped();
        let scale_L = T::cbrt(
            T::one()
                / T::max(
                    T::max(rgb_scale.red, rgb_scale.green),
                    T::max(rgb_scale.blue, T::zero()),
                ),
        );

        L = L / scale_L;
        C = C / scale_L;

        // use L_r instead of L and also scale C by L_r/L
        let L_r = ok_utils::toe(L);
        C = C * L_r / L;
        L = L_r;

        // we can now compute v and s:
        let v = L / L_v;
        let s = (S_0 + ST_max.t) * C_v / ((ST_max.t * S_0) + ST_max.t * k * C_v);

        Self::new(h, s, v)
    }
}
impl<T> FromColorUnclamped<Okhwb<T>> for Okhsv<T>
where
    T: Real
        + AbsDiffEq
        + PartialOrd
        + MinMax
        + Copy
        + Powi
        + Sqrt
        + Cbrt
        + Arithmetics
        + Trigonometry
        + Zero
        + Hypot
        + One
        + FromScalar
        + Debug
        + RealAngle,
    T::Scalar: Real
        + Zero
        + One
        + Recip
        + Hypot
        + IsValidDivisor<Mask = bool>
        + Arithmetics
        + Clone
        + FromScalar<Scalar = T::Scalar>
        + Debug,
{
    fn from_color_unclamped(hwb: Okhwb<T>) -> Self {
        if hwb.blackness == T::one() {
            return Self::new(hwb.hue, T::zero(), T::zero());
        }
        Self::new(
            hwb.hue,
            T::one() - hwb.whiteness / (T::one() - hwb.blackness),
            T::one() - hwb.blackness,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::convert::FromColorUnclamped;
    use crate::rgb::Rgb;
    use crate::{encoding, LinSrgb, Okhsv, Oklab, Srgb};
    use std::str::FromStr;

    #[test]
    fn test_roundtrip_okhsv_oklab_is_original() {
        let colors = [
            (
                "red",
                Oklab::from_color_unclamped(LinSrgb::new(1.0, 0.0, 0.0)),
            ),
            (
                "green",
                Oklab::from_color_unclamped(LinSrgb::new(0.0, 1.0, 0.0)),
            ),
            (
                "cyan",
                Oklab::from_color_unclamped(LinSrgb::new(0.0, 1.0, 1.0)),
            ),
            (
                "magenta",
                Oklab::from_color_unclamped(LinSrgb::new(1.0, 0.0, 1.0)),
            ),
            (
                "white",
                Oklab::from_color_unclamped(LinSrgb::new(1.0, 1.0, 1.0)),
            ),
            (
                "black",
                Oklab::from_color_unclamped(LinSrgb::new(0.0, 0.0, 0.0)),
            ),
            (
                "grey",
                Oklab::from_color_unclamped(LinSrgb::new(0.5, 0.5, 0.5)),
            ),
            (
                "yellow",
                Oklab::from_color_unclamped(LinSrgb::new(1.0, 1.0, 0.0)),
            ),
            (
                "blue",
                Oklab::from_color_unclamped(LinSrgb::new(0.0, 0.0, 1.0)),
            ),
        ];
        for (name, color) in colors {
            let rgb: Rgb<encoding::Srgb, u8> =
                crate::Srgb::<f32>::from_color_unclamped(color).into_format();
            println!(
                "\n\
            roundtrip of {name} (#{:x} / {:?})\n\
            =================================================",
                rgb, color
            );

            let okhsv = Okhsv::from_color_unclamped(color);
            println!("Okhsv: {:?}", okhsv);
            let roundtrip_color = Oklab::from_color_unclamped(okhsv);
            assert!(
                relative_eq!(roundtrip_color, color, epsilon = 1e-3),
                "'{name}' failed. {:?} != {:?}",
                roundtrip_color,
                color
            );
        }
    }

    #[test]
    fn test_srgb_to_okhsv() {
        let red_hex = "#ff0004";
        let rgb: Srgb = Rgb::<encoding::Srgb, _>::from_str(red_hex)
            .unwrap()
            .into_format();
        let oklab = Oklab::from_color_unclamped(rgb);
        let okhsv = Okhsv::from_color_unclamped(oklab);
        assert_relative_eq!(okhsv.saturation, 1.0, epsilon = 1e-3);
        assert_relative_eq!(okhsv.value, 1.0, epsilon = 1e-3);
        assert_relative_eq!(
            okhsv.hue.into_raw_degrees(),
            29.0,
            epsilon = 1e-3,
            max_relative = 1e-3
        );
    }

    #[test]
    fn test_okhsv_to_srgb() {
        let okhsv = Okhsv::new(0.0_f32, 0.5, 0.5);
        let oklab = Oklab::from_color_unclamped(okhsv);
        let rgb = Srgb::from_color_unclamped(oklab);
        let rgb8: Rgb<encoding::Srgb, u8> = rgb.into_format();
        let hex_str = format!("{:x}", rgb8);
        assert_eq!(hex_str, "7a4355");
    }
}
