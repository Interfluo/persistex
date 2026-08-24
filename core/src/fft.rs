//! Radix-2 FFT and the minimal complex arithmetic it needs.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Complex number. Hand-rolled so the crate keeps zero dependencies.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cx {
    pub re: f64,
    pub im: f64,
}

impl Cx {
    pub const ZERO: Cx = Cx { re: 0.0, im: 0.0 };

    pub fn new(re: f64, im: f64) -> Self {
        Cx { re, im }
    }

    /// exp(i*theta), scaled by `r`.
    pub fn polar(r: f64, theta: f64) -> Self {
        Cx {
            re: r * theta.cos(),
            im: r * theta.sin(),
        }
    }

    pub fn conj(self) -> Self {
        Cx {
            re: self.re,
            im: -self.im,
        }
    }

    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
}

impl std::ops::Add for Cx {
    type Output = Cx;
    fn add(self, o: Cx) -> Cx {
        Cx {
            re: self.re + o.re,
            im: self.im + o.im,
        }
    }
}

impl std::ops::Sub for Cx {
    type Output = Cx;
    fn sub(self, o: Cx) -> Cx {
        Cx {
            re: self.re - o.re,
            im: self.im - o.im,
        }
    }
}

impl std::ops::Mul for Cx {
    type Output = Cx;
    fn mul(self, o: Cx) -> Cx {
        Cx {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }
}

/// Scaling by a real, used to normalise the inverse transform.
impl std::ops::Mul<f64> for Cx {
    type Output = Cx;
    fn mul(self, k: f64) -> Cx {
        Cx {
            re: self.re * k,
            im: self.im * k,
        }
    }
}

/// Twiddle tables, keyed by transform length and direction.
type TwiddleCache = HashMap<(usize, bool), Rc<Vec<Cx>>>;

thread_local! {
    static TWIDDLES: RefCell<TwiddleCache> = RefCell::new(HashMap::new());
}

fn twiddles(n: usize, inverse: bool) -> Rc<Vec<Cx>> {
    TWIDDLES.with(|cache| {
        let mut map = cache.borrow_mut();
        map.entry((n, inverse))
            .or_insert_with(|| {
                let sign = if inverse { 2.0 } else { -2.0 };
                Rc::new(
                    (0..n / 2)
                        .map(|m| Cx::polar(1.0, sign * std::f64::consts::PI * m as f64 / n as f64))
                        .collect(),
                )
            })
            .clone()
    })
}

pub fn next_power_of_two(n: usize) -> usize {
    let mut p = 1usize;
    while p < n {
        p <<= 1;
    }
    p
}

/// In-place iterative Cooley-Tukey FFT. `a.len()` must be a power of two.
pub fn transform(a: &mut [Cx], inverse: bool) {
    let n = a.len();
    assert!(n.is_power_of_two(), "FFT length must be a power of two");
    if n < 2 {
        return;
    }

    // bit-reversal permutation
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            a.swap(i, j);
        }
    }

    let table = twiddles(n, inverse);
    let mut length = 2usize;
    while length <= n {
        let half = length >> 1;
        let step = n / length;
        for k in 0..half {
            // hoisted: one twiddle lookup per k, not per block
            let w = table[k * step];
            let mut lo = k;
            while lo < n {
                let hi = lo + half;
                let u = a[lo];
                let v = a[hi] * w;
                a[lo] = u + v;
                a[hi] = u - v;
                lo += length;
            }
        }
        length <<= 1;
    }

    if inverse {
        let inv = 1.0 / n as f64;
        for value in a.iter_mut() {
            *value = *value * inv;
        }
    }
}
