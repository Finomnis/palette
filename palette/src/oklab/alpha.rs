use crate::alpha::Alpha;
use crate::oklab::Oklab;

/// Oklab with an alpha component.
pub type Oklaba<T = f32> = Alpha<Oklab<T>, T>;

///<span id="Oklaba"></span>[`Oklaba`](crate::Oklaba) implementations.
impl<T, A> Alpha<Oklab<T>, A> {
    /// Create an Oklab color with transparency.
    pub const fn new(l: T, a: T, b: T, alpha: A) -> Self {
        Alpha {
            color: Oklab::new(l, a, b),
            alpha,
        }
    }

    /// Convert to a `(L, a, b, alpha)` tuple.
    pub fn into_components(self) -> (T, T, T, A) {
        (self.color.l, self.color.a, self.color.b, self.alpha)
    }

    /// Convert from a `(L, a, b, alpha)` tuple.
    pub fn from_components((l, a, b, alpha): (T, T, T, A)) -> Self {
        Self::new(l, a, b, alpha)
    }
}

impl<T, A> From<(T, T, T, A)> for Alpha<Oklab<T>, A> {
    fn from(components: (T, T, T, A)) -> Self {
        Self::from_components(components)
    }
}

impl<T, A> From<Alpha<Oklab<T>, A>> for (T, T, T, A) {
    fn from(color: Alpha<Oklab<T>, A>) -> (T, T, T, A) {
        color.into_components()
    }
}
