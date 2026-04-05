// Vendored from ordpath 0.5.0 — https://github.com/yohdeadfall/ordpath/
// License: MIT OR Apache-2.0 — Author: Yoh Deadfall <yoh.deadfall@hotmail.com>
//
// Original crate by Yoh Deadfall. Vendored into fspulse to add HierarchyId
// layer and remove the external dependency.

//! A hierarchy labeling scheme called ORDPATH.

#![allow(missing_docs)]

use std::alloc::{self, Layout};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt::{self, Debug, Display};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::iter::FusedIterator;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ptr::NonNull;
use std::str::FromStr;
use std::{mem, slice};

pub(crate) mod enc;
mod error;
mod reader;
mod writer;

pub use enc::*;
pub use error::*;
pub use reader::*;
pub use writer::*;

const USIZE_BYTES: usize = (usize::BITS / u8::BITS) as usize;

union Data<const N: usize> {
    inline: MaybeUninit<[u8; N]>,
    heap: NonNull<u8>,
}

impl<const N: usize> Data<N> {
    const fn new_inline() -> Self {
        Self {
            inline: MaybeUninit::uninit(),
        }
    }

    const fn new_heap(data: NonNull<u8>) -> Self {
        Self { heap: data }
    }
}

#[repr(C)]
pub(crate) struct Buf<const N: usize> {
    #[cfg(target_endian = "little")]
    size: usize,
    data: Data<N>,
    #[cfg(target_endian = "big")]
    size: usize,
}

impl<const N: usize> Buf<N> {
    const INLINE_SIZE_LEN: usize = {
        const fn max(lhs: usize, rhs: usize) -> usize {
            if lhs > rhs {
                lhs
            } else {
                rhs
            }
        }

        const fn meta_size(data_len: usize) -> usize {
            (usize::BITS - data_len.leading_zeros())
                .saturating_add(1)
                .div_ceil(u8::BITS) as usize
        }

        let data = max(N, size_of::<NonNull<u8>>());
        let data = size_of::<Buf<N>>() - meta_size(data);

        meta_size(data)
    };

    const INLINE_SIZE_MASK: usize =
        (usize::MAX >> (usize::BITS - u8::BITS * Self::INLINE_SIZE_LEN as u32));

    const INLINE_DATA_LEN: usize = size_of::<Self>() - Self::INLINE_SIZE_LEN;
    const INLINE_DATA_POS: usize = if cfg!(target_endian = "little") {
        size_of::<usize>() - Self::INLINE_SIZE_LEN
    } else {
        0
    };

    fn new(bits: usize) -> Result<Self, Error> {
        if bits > usize::MAX >> 1 {
            return Err(Error::new(ErrorKind::CapacityOverflow));
        }

        let bytes = bits.div_ceil(8);
        let spilled = bytes >= Self::INLINE_DATA_LEN;

        Ok(Self {
            size: (bits << 1) | spilled as usize,
            data: if spilled {
                let layout = Layout::array::<u8>(bytes).unwrap();
                let ptr = NonNull::new(unsafe { alloc::alloc(layout) }).unwrap();

                Data::new_heap(ptr)
            } else {
                Data::new_inline()
            },
        })
    }

    #[inline]
    const fn spilled(&self) -> bool {
        self.size & 1 == 1
    }

    #[inline]
    const fn len_in_bits(&self) -> usize {
        let size = if self.spilled() {
            self.size
        } else {
            self.size & Self::INLINE_SIZE_MASK
        };
        size >> 1
    }

    #[inline]
    const fn len_in_bytes(&self) -> usize {
        self.len_in_bits().div_ceil(8)
    }

    #[inline]
    const fn as_ptr(&self) -> *const u8 {
        unsafe {
            if self.spilled() {
                self.data.heap.as_ptr()
            } else {
                // TODO: replace by transpose when maybe_uninit_uninit_array_transpose stabilized.
                // https://github.com/rust-lang/rust/issues/96097
                (&raw const self.data.inline)
                    .cast::<u8>()
                    .byte_sub(Self::INLINE_DATA_POS)
            }
        }
    }

    #[inline]
    fn as_mut_ptr(&mut self) -> *mut u8 {
        unsafe {
            if self.spilled() {
                self.data.heap.as_ptr()
            } else {
                // TODO: replace by transpose when maybe_uninit_uninit_array_transpose stabilized.
                // https://github.com/rust-lang/rust/issues/96097
                (&raw mut self.data.inline)
                    .cast::<u8>()
                    .byte_sub(Self::INLINE_DATA_POS)
            }
        }
    }

    #[inline]
    const fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len_in_bytes()) }
    }

    #[inline]
    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len_in_bytes()) }
    }
}

impl<const N: usize> Drop for Buf<N> {
    fn drop(&mut self) {
        if self.spilled() {
            unsafe {
                let data = self.as_mut_slice();
                alloc::dealloc(
                    data.as_mut_ptr(),
                    Layout::from_size_align_unchecked(data.len(), align_of::<u8>()),
                );
            }
        }
    }
}

impl<const N: usize> PartialEq for Buf<N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_slice().eq(other.as_slice())
    }
}

impl<const N: usize> Eq for Buf<N> {}

impl<const N: usize> PartialOrd for Buf<N> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> Ord for Buf<N> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl<const N: usize> Clone for Buf<N> {
    fn clone(&self) -> Self {
        let mut other = Self::new(self.len_in_bits()).unwrap();
        other.as_mut_slice().clone_from_slice(self.as_slice());
        other
    }
}

/// A data type representing an ORDPATH is stored as a continuous sequence of bytes.
pub struct OrdPathBuf<E: Encoding = DefaultEncoding, const N: usize = USIZE_BYTES> {
    raw: Buf<N>,
    enc: E,
}

impl<E: Encoding, const N: usize> OrdPathBuf<E, N> {
    #[inline]
    fn new(bits: usize, enc: E) -> Result<Self, Error> {
        Ok(Self {
            raw: Buf::new(bits)?,
            enc,
        })
    }

    fn from_borrowed(s: &OrdPath<E, N>) -> Self
    where
        E: Clone,
    {
        let mut path = Self::new(s.bytes().len_in_bits(), s.encoding().clone()).unwrap();
        unsafe {
            // SAFETY: The buffer has exactly the same size as the slice.
            s.bytes()
                .read_exact(path.raw.as_mut_slice())
                .unwrap_unchecked();
        }
        path
    }

    /// Encodes a slice `s` to return a new `OrdPath` with the specified encoding.
    ///
    /// # Panics
    ///
    /// This function might panic if the given slice contains ordinals that are
    /// out of the range the provided encoding supports, or the resulting
    /// ORDPATH exceeds the maximum supported length.
    ///
    /// See also [`OrdPath::try_from_ordinals`] which will return an [`Error`] rather than panicking.
    #[inline]
    pub fn from_ordinals(s: &[i64], enc: E) -> Self {
        Self::try_from_ordinals(s, enc).unwrap()
    }

    /// Parses a string `s` to return a new `OrdPath` with the specified encoding.
    ///
    /// # Panics
    ///
    /// This function might panic if the given string contains any value other
    /// than numbers separated by dots, or it contains numbers that are out of
    /// the range the provided encoding supports, or the resulting ORDPATH
    /// exceeds the maximum supported length.
    ///
    /// See also [`OrdPath::try_from_str`] which will return an [`Error`] rather than panicking.
    #[inline]
    pub fn from_str(s: &str, enc: E) -> Self {
        Self::try_from_str(s, enc).unwrap()
    }

    /// Creates an `OrdPath` from a byte slice `s`.
    ///
    /// # Panics
    ///
    /// This function might panic if the given slice is not a valid ORDPATH, it
    /// cannot be read by the provided encoding, or the given slice exceeds the
    /// maximum supported length.
    ///
    /// See also [`OrdPath::try_from_bytes`] which will return an [`Error`] rather than panicking.
    #[inline]
    pub fn from_bytes(s: &[u8], enc: E) -> Self {
        Self::try_from_bytes(s, enc).unwrap()
    }

    /// Tries to encode a slice of ordinals `s` and create a new `OrdPath`.
    pub fn try_from_ordinals(s: &[i64], enc: E) -> Result<Self, Error> {
        let mut bits = 0isize;
        for ordinal in s {
            bits = bits.wrapping_add(
                enc.stage_by_value(*ordinal)
                    .ok_or_else(|| Error::new(ErrorKind::InvalidInput))?
                    .bits()
                    .into(),
            );
        }
        let mut path = Self::new(
            bits.try_into()
                .map_err(|_| Error::new(ErrorKind::CapacityOverflow))?,
            enc,
        )?;
        let mut writer = Writer::new(path.raw.as_mut_slice(), &path.enc);
        for ordinal in s {
            writer.write(*ordinal)?;
        }
        drop(writer);
        Ok(path)
    }

    /// Tries to parse a string `s` and create a new `OrdPath`.
    pub fn try_from_str(s: &str, enc: E) -> Result<Self, Error> {
        let mut v = Vec::new();
        for x in s.split_terminator('.') {
            v.push(x.parse::<i64>()?);
        }

        Self::try_from_ordinals(&v, enc)
    }

    /// Tries to create an `OrdPath` from a byte slice 's`.
    pub fn try_from_bytes(s: &[u8], enc: E) -> Result<Self, Error> {
        let mut bits = 0isize;
        let mut reader = Reader::new(s, &enc);
        while let Some((_, stage)) = reader.read()? {
            bits = bits.wrapping_add(stage.bits().into());
        }
        let mut path = Self::new(
            bits.try_into()
                .map_err(|_| Error::new(ErrorKind::CapacityOverflow))?,
            enc,
        )?;
        path.raw.as_mut_slice().copy_from_slice(s);
        Ok(path)
    }

    /// Returns a reference to the used encoding.
    #[inline]
    pub fn encoding(&self) -> &E {
        &self.enc
    }
}

impl<E: Encoding, const N: usize> AsRef<[u8]> for OrdPathBuf<E, N> {
    fn as_ref(&self) -> &[u8] {
        self.raw.as_slice()
    }
}

impl<E: Encoding, const N: usize> Borrow<OrdPath<E, N>> for OrdPathBuf<E, N> {
    fn borrow(&self) -> &OrdPath<E, N> {
        self
    }
}

impl<E: Encoding, const N: usize> Deref for OrdPathBuf<E, N> {
    type Target = OrdPath<E, N>;

    fn deref(&self) -> &Self::Target {
        OrdPath::new(self, self.raw.len_in_bits())
    }
}

impl<E: Encoding, const N: usize> PartialEq for OrdPathBuf<E, N>
where
    E: PartialEq,
{
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.encoding().eq(other.encoding()) && self.raw.eq(&other.raw)
    }
}

impl<E: Encoding, const N: usize> PartialOrd for OrdPathBuf<E, N>
where
    E: PartialEq,
{
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.encoding()
            .eq(other.encoding())
            .then(|| self.bytes().cmp(other.bytes()))
    }
}

impl<E: Encoding, const N: usize> Hash for OrdPathBuf<E, N> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.as_ref());
    }
}

impl<E: Encoding, const N: usize> Clone for OrdPathBuf<E, N>
where
    E: Clone,
{
    #[inline]
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
            enc: self.enc.clone(),
        }
    }
}

impl<E: Encoding, const N: usize> Debug for OrdPathBuf<E, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

impl<E: Encoding, const N: usize> Display for OrdPathBuf<E, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ordinals = self.ordinals();
        if let Some(value) = ordinals.next() {
            write!(f, "{}", value)?;
            for value in ordinals {
                write!(f, ".{}", value)?;
            }
        }
        Ok(())
    }
}

impl<E: Encoding + Default, const N: usize> FromStr for OrdPathBuf<E, N> {
    type Err = Error;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s, Default::default())
    }
}

/// A slice of an [`OrdPath`].
pub struct OrdPath<E: Encoding, const N: usize> {
    data: [OrdPathBuf<E, N>],
}

impl<E: Encoding, const N: usize> OrdPath<E, N> {
    #[inline]
    fn new(path: &OrdPathBuf<E, N>, bits: usize) -> &Self {
        unsafe { mem::transmute(slice::from_raw_parts(path as *const OrdPathBuf<E, N>, bits)) }
    }

    #[inline]
    fn path(&self) -> &OrdPathBuf<E, N> {
        unsafe { &*self.data.as_ptr() }
    }

    /// Returns a reference to the used encoding.
    #[inline]
    pub fn encoding(&self) -> &E {
        self.path().encoding()
    }

    /// Produces an iterator over the bytes of an `OrdPath`.
    #[inline]
    pub fn bytes(&self) -> &Bytes {
        unsafe { Bytes::from_raw_parts(self.path().as_ref().as_ptr(), self.data.len()) }
    }

    /// Produces an iterator over the ordinals of an `OrdPath`.
    #[inline]
    pub fn ordinals(&self) -> Ordinals<&Bytes, &E> {
        Ordinals {
            reader: Reader::new(self.bytes(), self.encoding()),
        }
    }

    /// Produces an iterator over `OrdPath` ancestors.
    #[inline]
    pub fn ancestors(&self) -> Ancestors<'_, E, N> {
        Ancestors { path: Some(self) }
    }

    /// Returns the `OrdPath` without its final element, if there is one.
    pub fn parent(&self) -> Option<&OrdPath<E, N>> {
        self.ancestors().next()
    }

    /// Returns `true` if `self` is an ancestor of `other`.
    #[inline]
    pub fn is_ancestor_of(&self, other: &Self) -> bool
    where
        E: PartialEq,
    {
        self.encoding().eq(other.encoding()) && self.bytes().is_ancestor(other.bytes())
    }

    /// Returns `true` if `self` is an descendant of `other`.
    #[inline]
    pub fn is_descendant_of(&self, other: &Self) -> bool
    where
        E: PartialEq,
    {
        other.is_ancestor_of(self)
    }
}

impl<E: Encoding + Clone, const N: usize> ToOwned for OrdPath<E, N> {
    type Owned = OrdPathBuf<E, N>;

    #[inline]
    fn to_owned(&self) -> OrdPathBuf<E, N> {
        OrdPathBuf::from_borrowed(self)
    }
}

impl<E: Encoding + PartialEq, const N: usize> PartialEq for OrdPath<E, N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other)
            .is_some_and(|r| r == Ordering::Equal)
    }
}

impl<E: Encoding + PartialEq, const N: usize> PartialOrd for OrdPath<E, N> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.encoding()
            .eq(other.encoding())
            .then(|| self.bytes().cmp(other.bytes()))
    }
}

impl<E: Encoding, const N: usize> Hash for OrdPath<E, N> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        for b in self.bytes() {
            state.write_u8(b);
        }
    }
}

impl<E: Encoding, const N: usize> Debug for OrdPath<E, N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

impl<E: Encoding, const N: usize> Display for OrdPath<E, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ordinals = self.ordinals();
        if let Some(value) = ordinals.next() {
            write!(f, "{}", value)?;
            for value in ordinals {
                write!(f, ".{}", value)?;
            }
        }
        Ok(())
    }
}

/// An iterator over the bytes of an [`OrdPath`].
///
/// This struct is created by the [`bytes`] method on [`OrdPath`].
/// See its documentation for more.
///
/// [`bytes`]: OrdPath::bytes
#[repr(transparent)]
pub struct Bytes {
    data: [u8],
}

impl Bytes {
    #[inline]
    unsafe fn from_raw_parts<'a>(ptr: *const u8, bits: usize) -> &'a Self {
        unsafe { mem::transmute(slice::from_raw_parts(ptr, bits)) }
    }

    #[inline]
    const fn len_in_bits(&self) -> usize {
        self.data.len()
    }

    #[inline]
    const fn len_in_bytes(&self) -> usize {
        self.len_in_bits().div_ceil(8)
    }

    unsafe fn get_at_unchecked(&self, idx: usize) -> u8 {
        unsafe { self.data.as_ptr().add(idx).read() }
    }

    unsafe fn get_slice_unchecked(&self, len: usize) -> &[u8] {
        unsafe { slice::from_raw_parts(self.data.as_ptr(), len) }
    }

    fn is_ancestor(&self, other: &Self) -> bool {
        self.len_in_bits() < other.len_in_bits()
            && match self.len_in_bytes() {
                0 => true,
                bytes => unsafe {
                    // SAFETY: The size is verified in the code above.
                    let last = bytes - 1;
                    self.get_slice_unchecked(last)
                        .eq(other.get_slice_unchecked(last))
                        && high_bits_eq(
                            self.len_in_bits(),
                            self.get_at_unchecked(last),
                            other.get_at_unchecked(last),
                        )
                },
            }
    }

    #[inline]
    fn ancestor<E: Encoding>(&self, enc: &E, nth: usize) -> Option<&Self> {
        const FORWARD_BUF_LEN: usize = size_of::<usize>();
        let mut bytes = self;
        for _ in 0..=nth.div_ceil(FORWARD_BUF_LEN) {
            if bytes.len_in_bits() == 0 {
                return None;
            }
            let mut idx = 0;
            let mut buf = [0u8; FORWARD_BUF_LEN];
            let mut bits = 0;
            let mut reader = Reader::new(bytes, enc);
            while let Some((_, stage)) = reader.read().unwrap() {
                bits += stage.bits() as usize;
                buf[idx % buf.len()] = stage.bits();
                idx = idx.wrapping_add(1);
            }
            for _ in 0..=buf.len().min(nth % FORWARD_BUF_LEN) {
                idx = idx.wrapping_sub(1);
                bits -= buf[idx % buf.len()] as usize;
            }
            bytes = unsafe { Bytes::from_raw_parts(bytes.data.as_ptr(), bits) };
        }
        Some(bytes)
    }
}

impl Read for &Bytes {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        unsafe {
            let mut data = self.get_slice_unchecked(self.len_in_bytes());
            let read = data.read(buf)?;
            if read > 0 && data.is_empty() {
                *buf.get_unchecked_mut(read - 1) &= high_bits_mask(self.len_in_bits());
            }

            *self =
                Bytes::from_raw_parts(data.as_ptr(), self.len_in_bits().saturating_sub(read * 8));

            Ok(read)
        }
    }
}

trait UncheckedIterator: Iterator {
    unsafe fn next_unchecked(&mut self) -> Self::Item;
    unsafe fn next_back_unchecked(&mut self) -> Self::Item;
}

impl UncheckedIterator for &Bytes {
    #[inline(always)]
    unsafe fn next_unchecked(&mut self) -> Self::Item {
        unsafe {
            let item = self.get_at_unchecked(0);
            let bits = self.len_in_bits();
            let (bits, item) = if bits >= 8 {
                (bits - 8, item)
            } else {
                (0, item & high_bits_mask(bits))
            };

            *self = Bytes::from_raw_parts(self.data.as_ptr().add(1), bits);

            item
        }
    }

    #[inline(always)]
    unsafe fn next_back_unchecked(&mut self) -> Self::Item {
        unsafe {
            let item = self.get_at_unchecked(self.len_in_bytes() - 1);
            let bits = self.len_in_bits();
            let (bits, item) = if bits & 7 == 0 {
                (bits - 8, item)
            } else {
                (bits & (!7), item & high_bits_mask(self.len_in_bits()))
            };

            *self = Bytes::from_raw_parts(self.data.as_ptr(), bits);

            item
        }
    }
}

impl Iterator for &Bytes {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.len_in_bits() == 0 {
                None
            } else {
                Some(self.next_unchecked())
            }
        }
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        unsafe {
            let bytes = self.len_in_bytes();
            if bytes <= n {
                *self = Bytes::from_raw_parts(self.data.as_ptr().add(bytes), 0);

                None
            } else {
                *self =
                    Bytes::from_raw_parts(self.data.as_ptr().add(n), self.len_in_bits() - n * 8);

                Some(self.next_unchecked())
            }
        }
    }

    #[inline]
    fn last(mut self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.next_back()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for &Bytes {
    fn next_back(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.len_in_bits() == 0 {
                None
            } else {
                Some(self.next_back_unchecked())
            }
        }
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        unsafe {
            let bytes = self.len_in_bytes();
            if bytes <= n {
                *self = Bytes::from_raw_parts(self.data.as_ptr(), 0);

                None
            } else {
                *self = Bytes::from_raw_parts(self.data.as_ptr(), self.len_in_bits() - n * 8);

                Some(self.next_back_unchecked())
            }
        }
    }
}

impl ExactSizeIterator for &Bytes {
    #[inline]
    fn len(&self) -> usize {
        self.len_in_bytes()
    }
}

impl FusedIterator for &Bytes {}

impl PartialEq for &Bytes {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.len_in_bits() == other.len_in_bits()
            && match self.len_in_bytes() {
                0 => true,
                bytes => {
                    unsafe {
                        // SAFETY: The size is verified in the code above.
                        let last = bytes - 1;
                        self.get_slice_unchecked(last) == other.get_slice_unchecked(last)
                            && high_bits_eq(
                                self.len_in_bits(),
                                self.get_at_unchecked(last),
                                other.get_at_unchecked(last),
                            )
                    }
                }
            }
    }
}

impl Eq for &Bytes {}

impl PartialOrd for &Bytes {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for &Bytes {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        fn cmp_impl(shortest: &Bytes, longest: &Bytes) -> Ordering {
            match shortest.len_in_bytes() {
                0 => Ordering::Equal,
                bytes => unsafe {
                    // SAFETY: The size is verified in the code above.
                    let last = bytes - 1;
                    shortest
                        .get_slice_unchecked(last)
                        .cmp(longest.get_slice_unchecked(last))
                        .then_with(|| {
                            let left = shortest.get_at_unchecked(last)
                                & high_bits_mask(shortest.len_in_bits());
                            let right = longest.get_at_unchecked(last)
                                & high_bits_mask(if bytes * 8 < longest.len_in_bits() {
                                    0
                                } else {
                                    longest.len_in_bits()
                                });

                            left.cmp(&right)
                        })
                },
            }
        }

        let ord = self.len_in_bits().cmp(&other.len_in_bits());
        if ord.is_le() {
            cmp_impl(self, other)
        } else {
            cmp_impl(other, self).reverse()
        }
        .then(ord)
    }
}

/// An iterator over the ordinals of an [`OrdPath`].
///
/// This struct is created by the [`ordinals`] method on [`OrdPath`].
/// See its documentation for more.
///
/// [`ordinals`]: OrdPath::ordinals
pub struct Ordinals<R: Read, E: Encoding> {
    reader: Reader<R, E>,
}

impl<R: Read, E: Encoding> Iterator for Ordinals<R, E> {
    type Item = i64;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.reader.read().unwrap().map(|x| x.0)
    }
}

impl<R: Read, E: Encoding> FusedIterator for Ordinals<R, E> {}

/// An iterator over `OrdPath` ancestors.
pub struct Ancestors<'a, E: Encoding, const N: usize> {
    path: Option<&'a OrdPath<E, N>>,
}

impl<'a, E: Encoding, const N: usize> Iterator for Ancestors<'a, E, N> {
    type Item = &'a OrdPath<E, N>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.path = self.path.and_then(|p| {
            p.bytes()
                .ancestor(p.encoding(), n)
                .map(|b| OrdPath::new(p.path(), b.len_in_bits()))
        });
        self.path
    }
}

impl<E: Encoding, const N: usize> FusedIterator for Ancestors<'_, E, N> {}

#[inline]
fn high_bits_eq(bits: usize, lhs: u8, rhs: u8) -> bool {
    let mask = high_bits_mask(bits);
    lhs & mask == rhs & mask
}

#[inline]
fn high_bits_mask(bits: usize) -> u8 {
    u8::MAX << (((usize::MAX << 3) - bits) & 7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_from_ordinals() {
        fn assert(s: &[i64]) {
            assert_eq!(
                <OrdPathBuf>::from_ordinals(s, DefaultEncoding)
                    .ordinals()
                    .collect::<Vec<_>>(),
                s
            );
        }

        assert(&[0; 0]);
        assert(&[0]);
        assert(&[0, 8]);
        assert(&[4440, 4440, 4440, 8]);
        assert(&[4440, 4440, 4440, 8, 0]);
        assert(&[4440, 4440, 4440, 4440]);
        assert(&[4440, 4440, 4440, 4440, 88]);
        assert(&[4295037272, 4295037272]);
        assert(&[4295037272, 4295037272, 4440, 88]);
        assert(&[4295037272, 4295037272, 4440, 344]);
        assert(&[4295037272, 4295037272, 4440, 4440]);

        assert(&[3]);
        assert(&[3, 8 + 5]);
        assert(&[4440 + 13, 4440 + 179, 4440 + 7541, 8 + 11]);
        assert(&[4440 + 13, 4440 + 179, 4440 + 7541, 8 + 11, 3]);
        assert(&[4440 + 13, 4440 + 179, 4440 + 7541, 4440 + 123]);
        assert(&[4440 + 13, 4440 + 179, 4440 + 7541, 4440 + 123, 88 + 11]);
        assert(&[4295037272 + 31, 4295037272 + 6793]);
        assert(&[4295037272 + 31, 4295037272 + 6793, 4440 + 7541, 88 + 11]);
        assert(&[4295037272 + 31, 4295037272 + 6793, 4440 + 7541, 344 + 71]);
        assert(&[4295037272 + 31, 4295037272 + 6793, 4440 + 7541, 4440 + 123]);
    }

    #[test]
    fn path_from_str() {
        fn assert(s: &str, o: &[i64]) {
            assert_eq!(
                <OrdPathBuf>::try_from_str(s, DefaultEncoding),
                Ok(<OrdPathBuf>::from_ordinals(o, DefaultEncoding))
            );
        }

        fn assert_err(s: &str, e: Error) {
            assert_eq!(
                <OrdPathBuf>::try_from_str(s, DefaultEncoding),
                Err(e.clone())
            );
        }

        assert("", &[]);
        assert("0", &[0]);
        assert("1", &[1]);
        assert("1.2", &[1, 2]);
        assert_err("1.a", Error::new(ErrorKind::InvalidInput));
        assert_err("1_2", Error::new(ErrorKind::InvalidInput));
        assert_err("a", Error::new(ErrorKind::InvalidInput));
    }

    #[test]
    fn path_to_string() {
        fn assert(o: Vec<i64>, s: &str) {
            assert_eq!(
                <OrdPathBuf>::from_ordinals(&o, DefaultEncoding).to_string(),
                s
            );
        }

        assert(vec![], "");
        assert(vec![0], "0");
        assert(vec![1], "1");
        assert(vec![1, 2], "1.2");
    }

    #[test]
    fn path_clone() {
        fn assert(o: &[i64]) {
            assert_eq!(
                <OrdPathBuf>::from_ordinals(o, DefaultEncoding).clone(),
                <OrdPathBuf>::from_ordinals(o, DefaultEncoding)
            );
        }

        assert(&[]);
        assert(&[0]);
        assert(&[1]);
        assert(&[1, 2]);
    }

    #[test]
    fn path_ordering() {
        fn assert(lhs: &[i64], rhs: &[i64], o: Ordering) {
            assert_eq!(
                <OrdPathBuf>::from_ordinals(lhs, DefaultEncoding)
                    .partial_cmp(&<OrdPathBuf>::from_ordinals(rhs, DefaultEncoding)),
                Some(o)
            );
        }

        assert(&[0; 0], &[0; 0], Ordering::Equal);
        assert(&[0; 0], &[0], Ordering::Less);
        assert(&[0], &[0; 0], Ordering::Greater);
        assert(&[0], &[0], Ordering::Equal);
        assert(&[0], &[1], Ordering::Less);
        assert(&[0], &[0, 1], Ordering::Less);
        assert(&[0], &[69976, 69976], Ordering::Less);
        assert(&[0], &[4295037272, 4295037272], Ordering::Less);
    }

    #[test]
    fn path_is_ancestor() {
        fn assert(e: bool, a: &[i64], d: &[i64]) {
            let x = <OrdPathBuf>::from_ordinals(a, DefaultEncoding);
            let y = <OrdPathBuf>::from_ordinals(d, DefaultEncoding);

            assert_eq!(e, x.is_ancestor_of(&y));
            assert_eq!(e, y.is_descendant_of(&x));
        }

        assert(true, &[], &[0]);
        assert(true, &[0], &[0, 1]);
        assert(true, &[0, 1], &[0, 1, 2, 3]);
        assert(
            true,
            &[4295037272, 4295037272],
            &[4295037272, 4295037272, 1],
        );

        assert(false, &[0], &[]);
        assert(false, &[0, 1], &[0]);
        assert(false, &[0, 1, 2, 3], &[0, 1]);
        assert(
            false,
            &[4295037272, 4295037272, 1],
            &[4295037272, 4295037272],
        );

        assert(false, &[], &[]);
        assert(false, &[0], &[0]);
        assert(false, &[0], &[1]);
    }

    #[test]
    fn path_iter_fused() {
        fn assert<R: Read, E: Encoding>(mut iter: Ordinals<R, E>) {
            assert_eq!(iter.next(), Some(1));
            assert_eq!(iter.next(), Some(2));
            assert_eq!(iter.next(), None);
            assert_eq!(iter.next(), None);
        }

        assert(<OrdPathBuf>::from_ordinals(&[1, 2], DefaultEncoding).ordinals());
    }

    #[test]
    fn path_parent() {
        let ords = (i8::MIN..i8::MAX).map(|x| x as i64).collect::<Vec<_>>();
        for n in 1..ords.len() {
            let ords = &ords[..n];
            let path = <OrdPathBuf>::from_ordinals(ords, DefaultEncoding);
            assert_eq!(
                ords[..(ords.len() - 1)],
                path.parent()
                    .map(|p| p.ordinals().collect::<Vec<_>>())
                    .unwrap_or_default()
            );
        }
    }

    #[test]
    fn bytes_next() {
        let path = <OrdPathBuf>::from_ordinals(&(0..5).collect::<Vec<_>>(), DefaultEncoding);
        assert_eq!(
            path.bytes().collect::<Vec<_>>(),
            path.as_ref().to_vec()
        );
    }

    #[test]
    fn bytes_next_back() {
        let path = <OrdPathBuf>::from_ordinals(&(0..5).collect::<Vec<_>>(), DefaultEncoding);
        assert_eq!(
            path.bytes().rev().collect::<Vec<_>>(),
            path.as_ref().iter().copied().rev().collect::<Vec<_>>()
        );
    }

    #[test]
    fn bytes_nth() {
        let nth = 5;
        let len = (0..nth as i64).reduce(|l, n| l + n).unwrap();
        let path = <OrdPathBuf>::from_ordinals(&(0..len).collect::<Vec<_>>(), DefaultEncoding);

        let mut actual = path.bytes();
        let mut expected = path.as_ref().iter().copied();

        for n in 0..=nth {
            assert_eq!(actual.nth(n), expected.nth(n));
        }
    }

    #[test]
    fn bytes_nth_back() {
        let nth = 5;
        let len = (0..=nth as i64).reduce(|l, n| l + n).unwrap();
        let path = <OrdPathBuf>::from_ordinals(&(0..len).collect::<Vec<_>>(), DefaultEncoding);

        let mut actual = path.bytes();
        let mut expected = path.as_ref().iter().copied();

        for n in 0..=nth {
            assert_eq!(actual.nth_back(n), expected.nth_back(n));
        }
    }
}
