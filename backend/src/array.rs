use std::{
    mem,
    ops::{Deref, Index},
};

/// A fixed size array.
#[derive(Debug)]
pub struct Array<T, const N: usize> {
    inner: [Option<T>; N],
    len: usize,
}

impl<T, const N: usize> Array<T, N> {
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: [const { None }; N],
            len: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, value: T) {
        assert!(self.len < N);
        let index = self.len;
        self.len += 1;
        self.inner[index] = Some(value);
    }

    #[inline]
    pub fn remove(&mut self, index: usize) {
        assert!(index < self.len);
        self.inner[index] = None;
        self.len -= 1;
        for i in index..N.saturating_sub(1) {
            self.inner[i] = self.inner[i + 1].take();
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn iter(&self) -> ArrayIterator<'_, T, N> {
        self.into_iter()
    }
}

impl<T: Clone, const N: usize> Clone for Array<T, N> {
    fn clone(&self) -> Self {
        let mut array = Array::<T, N>::new();
        for item in self {
            array.push(item.clone());
        }
        array
    }
}

impl<T: Copy, const N: usize> Copy for Array<T, N> {}

impl<T: PartialEq, const N: usize> PartialEq for Array<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T, const N: usize> Deref for Array<T, N> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // SAFETY: `Option<T>` can be safely transmuted to `T` as part of Rust guaranteed
        unsafe { mem::transmute::<&[Option<T>], &[T]>(&self.inner[0..self.len]) }
    }
}

impl<T, const N: usize> Index<usize> for Array<T, N> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.inner[index].as_ref().unwrap()
    }
}

impl<T: Eq, const N: usize> Eq for Array<T, N> {}

impl<T, const N: usize> Default for Array<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A, const N: usize> FromIterator<A> for Array<A, N> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut array = Array::new();
        for elem in iter {
            array.push(elem);
        }
        array
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a Array<T, N> {
    type Item = &'a T;

    type IntoIter = ArrayIterator<'a, T, N>;

    fn into_iter(self) -> Self::IntoIter {
        ArrayIterator {
            array: self,
            index: 0,
        }
    }
}

impl<T, const N: usize> IntoIterator for Array<T, N> {
    type Item = T;

    type IntoIter = ArrayIntoIterator<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        ArrayIntoIterator {
            array: self,
            index: 0,
        }
    }
}

pub struct ArrayIterator<'a, T, const N: usize> {
    array: &'a Array<T, N>,
    index: usize,
}

impl<'a, T, const N: usize> Iterator for ArrayIterator<'a, T, N> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.array.len {
            return None;
        }
        let index = self.index;
        self.index += 1;
        self.array.inner[index].as_ref()
    }
}

pub struct ArrayIntoIterator<T, const N: usize> {
    array: Array<T, N>,
    index: usize,
}

impl<T, const N: usize> Iterator for ArrayIntoIterator<T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.array.len {
            return None;
        }
        let index = self.index;
        self.index += 1;
        self.array.inner[index].take()
    }
}

#[cfg(test)]
mod tests {
    use super::Array;

    #[test]
    fn push() {
        let mut array = Array::<u32, 1000>::new();
        let mut vec = Vec::new();
        for i in 0..1000 {
            array.push(i);
            vec.push(Some(i));
        }
        assert_eq!(array.len, 1000);
        assert_eq!(&array.inner, vec.as_slice());
    }

    #[test]
    fn into_iter() {
        let mut vec = Vec::new();
        for i in 0..1000 {
            vec.push(i);
        }
        let len = vec.len();
        let array = Array::<u32, 1000>::from_iter(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.into_iter().zip(0..1000) {
            assert_eq!(elem, i);
        }

        let mut vec = Vec::new();
        for i in 333..555 {
            vec.push(i);
        }
        let len = vec.len();
        let array = Array::<u32, 1000>::from_iter(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.into_iter().zip(333..555) {
            assert_eq!(elem, i);
        }
    }

    #[test]
    fn iter() {
        let mut vec = Vec::new();
        for i in 0..1000 {
            vec.push(i);
        }
        let len = vec.len();
        let array = Array::<u32, 1000>::from_iter(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.iter().zip(0..1000) {
            assert_eq!(elem, &i);
        }

        let mut vec = Vec::new();
        for i in 333..555 {
            vec.push(i);
        }
        let len = vec.len();
        let array = Array::<u32, 1000>::from_iter(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.iter().zip(333..555) {
            assert_eq!(elem, &i);
        }
    }
}
