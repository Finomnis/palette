use core::{
    marker::PhantomData,
    ops::{Add, AddAssign, BitAnd, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign},
};

#[cfg(feature = "approx")]
use approx::{AbsDiffEq, RelativeEq, UlpsEq};
#[cfg(feature = "random")]
use rand::{
    distributions::{
        uniform::{SampleBorrow, SampleUniform, Uniform, UniformSampler},
        Distribution, Standard,
    },
    Rng,
};

use crate::{
    angle::RealAngle,
    blend::{PreAlpha, Premultiply},
    bool_mask::{HasBoolMask, LazySelect},
    clamp, clamp_assign, contrast_ratio,
    convert::FromColorUnclamped,
    num::{
        self, Arithmetics, FromScalarArray, IntoScalarArray, IsValidDivisor, MinMax, One,
        PartialCmp, Powf, Powi, Real, Recip, Trigonometry, Zero,
    },
    stimulus::Stimulus,
    white_point::{WhitePoint, D65},
    Alpha, Clamp, ClampAssign, FromColor, GetHue, IsWithinBounds, Lchuv, Lighten, LightenAssign,
    LuvHue, Mix, MixAssign, RelativeContrast, Xyz,
};

/// CIE L\*u\*v\* (CIELUV) with an alpha component. See the [`Luva`
/// implementation in `Alpha`](crate::Alpha#Luva).
pub type Luva<Wp = D65, T = f32> = Alpha<Luv<Wp, T>, T>;

/// The CIE L\*u\*v\* (CIELUV) color space.
///
/// CIE L\*u\*v\* is a device independent color space. It is a simple
/// transformation of the CIE XYZ color space with the additional
/// property of being more perceptually uniform. In contrast to
/// CIELAB, CIELUV is also linear for a fixed lightness: additive
/// mixtures of colors (at a fixed lightness) will fall on a line in
/// CIELUV-space.
///
/// As a result, CIELUV is used more frequently for additive settings.
#[derive(Debug, ArrayCast, FromColorUnclamped, WithAlpha)]
#[cfg_attr(feature = "serializing", derive(Serialize, Deserialize))]
#[palette(
    palette_internal,
    white_point = "Wp",
    component = "T",
    skip_derives(Xyz, Luv, Lchuv)
)]
#[repr(C)]
pub struct Luv<Wp = D65, T = f32> {
    /// L\* is the lightness of the color. 0.0 gives absolute black and 100
    /// give the brightest white.
    pub l: T,

    /// The range of valid u\* varies depending on the values of L\*
    /// and v\*, but at its limits u\* is within the interval (-84.0,
    /// 176.0).
    pub u: T,

    /// The range of valid v\* varies depending on the values of L\*
    /// and u\*, but at its limits v\* is within the interval (-135.0,
    /// 108.0).
    pub v: T,

    /// The white point associated with the color's illuminant and observer.
    /// D65 for 2 degree observer is used by default.
    #[cfg_attr(feature = "serializing", serde(skip))]
    #[palette(unsafe_zero_sized)]
    pub white_point: PhantomData<Wp>,
}

impl<Wp, T> Copy for Luv<Wp, T> where T: Copy {}

impl<Wp, T> Clone for Luv<Wp, T>
where
    T: Clone,
{
    fn clone(&self) -> Luv<Wp, T> {
        Luv {
            l: self.l.clone(),
            u: self.u.clone(),
            v: self.v.clone(),
            white_point: PhantomData,
        }
    }
}

impl<Wp, T> Luv<Wp, T> {
    /// Create a CIE L\*u\*v\* color.
    pub const fn new(l: T, u: T, v: T) -> Self {
        Luv {
            l,
            u,
            v,
            white_point: PhantomData,
        }
    }

    /// Convert to a `(L\*, u\*, v\*)` tuple.
    pub fn into_components(self) -> (T, T, T) {
        (self.l, self.u, self.v)
    }

    /// Convert from a `(L\*, u\*, v\*)` tuple.
    pub fn from_components((l, u, v): (T, T, T)) -> Self {
        Self::new(l, u, v)
    }
}

impl<Wp, T> Luv<Wp, T>
where
    T: Zero + Real,
{
    /// Return the `l` value minimum.
    pub fn min_l() -> T {
        T::zero()
    }

    /// Return the `l` value maximum.
    pub fn max_l() -> T {
        T::from_f64(100.0)
    }

    /// Return the `u` value minimum.
    pub fn min_u() -> T {
        T::from_f64(-84.0)
    }

    /// Return the `u` value maximum.
    pub fn max_u() -> T {
        T::from_f64(176.0)
    }

    /// Return the `v` value minimum.
    pub fn min_v() -> T {
        T::from_f64(-135.0)
    }

    /// Return the `v` value maximum.
    pub fn max_v() -> T {
        T::from_f64(108.0)
    }
}

///<span id="Luva"></span>[`Luva`](crate::Luva) implementations.
impl<Wp, T, A> Alpha<Luv<Wp, T>, A> {
    /// Create a CIE L\*u\*v\* color with transparency.
    pub const fn new(l: T, u: T, v: T, alpha: A) -> Self {
        Alpha {
            color: Luv::new(l, u, v),
            alpha,
        }
    }

    /// Convert to u `(L\*, u\*, v\*, alpha)` tuple.
    pub fn into_components(self) -> (T, T, T, A) {
        (self.color.l, self.color.u, self.color.v, self.alpha)
    }

    /// Convert from u `(L\*, u\*, v\*, alpha)` tuple.
    pub fn from_components((l, u, v, alpha): (T, T, T, A)) -> Self {
        Self::new(l, u, v, alpha)
    }
}

impl<Wp, T> FromColorUnclamped<Luv<Wp, T>> for Luv<Wp, T> {
    fn from_color_unclamped(color: Luv<Wp, T>) -> Self {
        color
    }
}

impl<Wp, T> FromColorUnclamped<Lchuv<Wp, T>> for Luv<Wp, T>
where
    T: RealAngle + Zero + MinMax + Trigonometry + Mul<Output = T> + Clone,
{
    fn from_color_unclamped(color: Lchuv<Wp, T>) -> Self {
        let (sin_hue, cos_hue) = color.hue.into_raw_radians().sin_cos();
        let chroma = color.chroma.max(T::zero());

        Luv::new(color.l, chroma.clone() * cos_hue, chroma * sin_hue)
    }
}

impl<Wp, T> FromColorUnclamped<Xyz<Wp, T>> for Luv<Wp, T>
where
    Wp: WhitePoint<T>,
    T: Real
        + Zero
        + Powi
        + Powf
        + Recip
        + Arithmetics
        + PartialOrd
        + Clone
        + HasBoolMask<Mask = bool>,
{
    fn from_color_unclamped(color: Xyz<Wp, T>) -> Self {
        let w = Wp::get_xyz();

        let kappa = T::from_f64(29.0 / 3.0).powi(3);
        let epsilon = T::from_f64(6.0 / 29.0).powi(3);

        let prime_denom =
            color.x.clone() + T::from_f64(15.0) * &color.y + T::from_f64(3.0) * color.z;
        if prime_denom == T::from_f64(0.0) {
            return Luv::new(T::zero(), T::zero(), T::zero());
        }
        let prime_denom_recip = prime_denom.recip();
        let prime_ref_denom_recip =
            (w.x.clone() + T::from_f64(15.0) * &w.y + T::from_f64(3.0) * w.z).recip();

        let u_prime: T = T::from_f64(4.0) * color.x * &prime_denom_recip;
        let u_ref_prime = T::from_f64(4.0) * w.x * &prime_ref_denom_recip;

        let v_prime: T = T::from_f64(9.0) * &color.y * prime_denom_recip;
        let v_ref_prime = T::from_f64(9.0) * &w.y * prime_ref_denom_recip;

        let y_r = color.y / w.y;
        let l = if y_r > epsilon {
            T::from_f64(116.0) * y_r.powf(T::from_f64(1.0 / 3.0)) - T::from_f64(16.0)
        } else {
            kappa * y_r
        };

        Luv {
            u: T::from_f64(13.0) * &l * (u_prime - u_ref_prime),
            v: T::from_f64(13.0) * &l * (v_prime - v_ref_prime),
            l,
            white_point: PhantomData,
        }
    }
}

impl<Wp, T> From<(T, T, T)> for Luv<Wp, T> {
    fn from(components: (T, T, T)) -> Self {
        Self::from_components(components)
    }
}

impl<Wp, T> From<Luv<Wp, T>> for (T, T, T) {
    fn from(color: Luv<Wp, T>) -> (T, T, T) {
        color.into_components()
    }
}

impl<Wp, T, A> From<(T, T, T, A)> for Alpha<Luv<Wp, T>, A> {
    fn from(components: (T, T, T, A)) -> Self {
        Self::from_components(components)
    }
}

impl<Wp, T, A> From<Alpha<Luv<Wp, T>, A>> for (T, T, T, A) {
    fn from(color: Alpha<Luv<Wp, T>, A>) -> (T, T, T, A) {
        color.into_components()
    }
}

impl_is_within_bounds! {
    Luv<Wp> {
        l => [Self::min_l(), Self::max_l()],
        u => [Self::min_u(), Self::max_u()],
        v => [Self::min_v(), Self::max_v()]
    }
    where T: Real + Zero
}

impl<Wp, T> Clamp for Luv<Wp, T>
where
    T: Zero + Real + num::Clamp,
{
    #[inline]
    fn clamp(self) -> Self {
        Self::new(
            clamp(self.l, Self::min_l(), Self::max_l()),
            clamp(self.u, Self::min_u(), Self::max_u()),
            clamp(self.v, Self::min_v(), Self::max_v()),
        )
    }
}

impl<Wp, T> ClampAssign for Luv<Wp, T>
where
    T: Zero + Real + num::ClampAssign,
{
    #[inline]
    fn clamp_assign(&mut self) {
        clamp_assign(&mut self.l, Self::min_l(), Self::max_l());
        clamp_assign(&mut self.u, Self::min_u(), Self::max_u());
        clamp_assign(&mut self.v, Self::min_v(), Self::max_v());
    }
}

impl_mix!(Luv<Wp>);
impl_lighten!(Luv<Wp> increase {l => [Self::min_l(), Self::max_l()]} other {u, v} phantom: white_point);
impl_premultiply!(Luv<Wp> {l, u, v} phantom: white_point);
impl_euclidean_distance!(Luv<Wp> {l, u, v});

impl<Wp, T> GetHue for Luv<Wp, T>
where
    T: RealAngle + Trigonometry + Add<T, Output = T> + Neg<Output = T> + Clone,
{
    type Hue = LuvHue<T>;

    fn get_hue(&self) -> LuvHue<T> {
        LuvHue::from_cartesian(self.u.clone(), self.v.clone())
    }
}

impl<Wp, T> HasBoolMask for Luv<Wp, T>
where
    T: HasBoolMask,
{
    type Mask = T::Mask;
}

impl<Wp, T> Default for Luv<Wp, T>
where
    T: Zero,
{
    fn default() -> Luv<Wp, T> {
        Luv::new(T::zero(), T::zero(), T::zero())
    }
}

impl_color_add!(Luv<Wp, T>, [l, u, v], white_point);
impl_color_sub!(Luv<Wp, T>, [l, u, v], white_point);
impl_color_mul!(Luv<Wp, T>, [l, u, v], white_point);
impl_color_div!(Luv<Wp, T>, [l, u, v], white_point);

impl_array_casts!(Luv<Wp, T>, [T; 3]);
impl_simd_array_conversion!(Luv<Wp>, [l, u, v], white_point);

impl_eq!(Luv<Wp>, [l, u, v]);

impl<Wp, T> RelativeContrast for Luv<Wp, T>
where
    T: Real + Arithmetics + PartialCmp,
    T::Mask: LazySelect<T>,
    Wp: WhitePoint<T>,
    Xyz<Wp, T>: FromColor<Self>,
{
    type Scalar = T;

    #[inline]
    fn get_contrast_ratio(self, other: Self) -> T {
        let xyz1 = Xyz::from_color(self);
        let xyz2 = Xyz::from_color(other);

        contrast_ratio(xyz1.y, xyz2.y)
    }
}

#[cfg(feature = "random")]
impl<Wp, T> Distribution<Luv<Wp, T>> for Standard
where
    T: Real + Mul<Output = T> + Sub<Output = T>,
    Standard: Distribution<T>,
{
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Luv<Wp, T> {
        Luv {
            l: rng.gen() * T::from_f64(100.0),
            u: rng.gen() * T::from_f64(260.0) - T::from_f64(84.0),
            v: rng.gen() * T::from_f64(243.0) - T::from_f64(135.0),
            white_point: PhantomData,
        }
    }
}

#[cfg(feature = "random")]
pub struct UniformLuv<Wp, T>
where
    T: SampleUniform,
{
    l: Uniform<T>,
    u: Uniform<T>,
    v: Uniform<T>,
    white_point: PhantomData<Wp>,
}

#[cfg(feature = "random")]
impl<Wp, T> SampleUniform for Luv<Wp, T>
where
    T: Clone + SampleUniform,
{
    type Sampler = UniformLuv<Wp, T>;
}

#[cfg(feature = "random")]
impl<Wp, T> UniformSampler for UniformLuv<Wp, T>
where
    T: Clone + SampleUniform,
{
    type X = Luv<Wp, T>;

    fn new<B1, B2>(low_v: B1, high_v: B2) -> Self
    where
        B1: SampleBorrow<Self::X> + Sized,
        B2: SampleBorrow<Self::X> + Sized,
    {
        let low = low_v.borrow().clone();
        let high = high_v.borrow().clone();

        UniformLuv {
            l: Uniform::new::<_, T>(low.l, high.l),
            u: Uniform::new::<_, T>(low.u, high.u),
            v: Uniform::new::<_, T>(low.v, high.v),
            white_point: PhantomData,
        }
    }

    fn new_inclusive<B1, B2>(low_v: B1, high_v: B2) -> Self
    where
        B1: SampleBorrow<Self::X> + Sized,
        B2: SampleBorrow<Self::X> + Sized,
    {
        let low = low_v.borrow().clone();
        let high = high_v.borrow().clone();

        UniformLuv {
            l: Uniform::new_inclusive::<_, T>(low.l, high.l),
            u: Uniform::new_inclusive::<_, T>(low.u, high.u),
            v: Uniform::new_inclusive::<_, T>(low.v, high.v),
            white_point: PhantomData,
        }
    }

    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Luv<Wp, T> {
        Luv {
            l: self.l.sample(rng),
            u: self.u.sample(rng),
            v: self.v.sample(rng),
            white_point: PhantomData,
        }
    }
}

#[cfg(feature = "bytemuck")]
unsafe impl<Wp, T> bytemuck::Zeroable for Luv<Wp, T> where T: bytemuck::Zeroable {}

#[cfg(feature = "bytemuck")]
unsafe impl<Wp: 'static, T> bytemuck::Pod for Luv<Wp, T> where T: bytemuck::Pod {}

#[cfg(test)]
mod test {
    use super::Luv;
    use crate::white_point::D65;
    use crate::{FromColor, LinSrgb};

    test_convert_into_from_xyz!(Luv);

    #[test]
    fn red() {
        let u = Luv::from_color(LinSrgb::new(1.0, 0.0, 0.0));
        let v = Luv::new(53.237116, 175.0098, 37.7650);
        assert_relative_eq!(u, v, epsilon = 0.01);
    }

    #[test]
    fn green() {
        let u = Luv::from_color(LinSrgb::new(0.0, 1.0, 0.0));
        let v = Luv::new(87.73703, -83.07975, 107.40136);
        assert_relative_eq!(u, v, epsilon = 0.01);
    }

    #[test]
    fn blue() {
        let u = Luv::from_color(LinSrgb::new(0.0, 0.0, 1.0));
        let v = Luv::new(32.30087, -9.40241, -130.35109);
        assert_relative_eq!(u, v, epsilon = 0.01);
    }

    #[test]
    fn ranges() {
        assert_ranges! {
            Luv<D65, f64>;
            clamped {
            l: 0.0 => 100.0,
            u: -84.0 => 176.0,
            v: -135.0 => 108.0
            }
            clamped_min {}
            unclamped {}
        }
    }
    /// Check that the arithmetic operations (add/sub) are all
    /// implemented.
    #[test]
    fn test_arithmetic() {
        let luv = Luv::<D65>::new(120.0, 40.0, 30.0);
        let luv2 = Luv::new(200.0, 30.0, 40.0);
        let mut _luv3 = luv + luv2;
        _luv3 += luv2;
        let mut _luv4 = luv2 + 0.3;
        _luv4 += 0.1;

        _luv3 = luv2 - luv;
        _luv3 = _luv4 - 0.1;
        _luv4 -= _luv3;
        _luv3 -= 0.1;
    }

    raw_pixel_conversion_tests!(Luv<D65>: l, u, v);
    raw_pixel_conversion_fail_tests!(Luv<D65>: l, u, v);

    #[test]
    fn check_min_max_components() {
        assert_relative_eq!(Luv::<D65, f32>::min_l(), 0.0);
        assert_relative_eq!(Luv::<D65, f32>::min_u(), -84.0);
        assert_relative_eq!(Luv::<D65, f32>::min_v(), -135.0);
        assert_relative_eq!(Luv::<D65, f32>::max_l(), 100.0);
        assert_relative_eq!(Luv::<D65, f32>::max_u(), 176.0);
        assert_relative_eq!(Luv::<D65, f32>::max_v(), 108.0);
    }

    #[cfg(feature = "serializing")]
    #[test]
    fn serialize() {
        let serialized = ::serde_json::to_string(&Luv::<D65>::new(80.0, 20.0, 30.0)).unwrap();

        assert_eq!(serialized, r#"{"l":80.0,"u":20.0,"v":30.0}"#);
    }

    #[cfg(feature = "serializing")]
    #[test]
    fn deserialize() {
        let deserialized: Luv = ::serde_json::from_str(r#"{"l":80.0,"u":20.0,"v":30.0}"#).unwrap();

        assert_eq!(deserialized, Luv::new(80.0, 20.0, 30.0));
    }

    #[cfg(feature = "random")]
    test_uniform_distribution! {
    Luv<D65, f32> {
    l: (0.0, 100.0),
    u: (-84.0, 176.0),
    v: (-135.0, 108.0)
    },
    min: Luv::new(0.0f32, -84.0, -135.0),
    max: Luv::new(100.0, 176.0, 108.0)
    }
}
