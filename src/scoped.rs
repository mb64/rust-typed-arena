use crate::Arena;
use crate::ChunkList;

use core::cell::{RefCell, RefMut};
use core::marker::PhantomData;
use core::mem;
use core::ops::Deref;

/// A scoped sub-arena
///
/// ```
/// use typed_arena::Arena;
/// use typed_arena::SubArena;
///
/// let arena = Arena::new();
/// // allocate some stuff from the arena
/// let x = arena.alloc(123);
/// let y = arena.alloc(456);
/// {
///     let sub_arena = SubArena::new(&arena);
///     // allocate more stuff from the arena
///     let z = sub_arena.alloc(789);
///     let w = sub_arena.alloc(4);
///
///     // when sub_arena is dropped, the arena is partially cleared
///     // z and w are dropped, but x and y are still good
/// }
/// assert_eq!(*x + *y, 123 + 456);
/// ```
///
/// While a `SubArena` is in use, the main `Arena` cannot be used -- trying to
/// do so results in a panic:
/// ```should_panic
/// use typed_arena::Arena;
/// use typed_arena::SubArena;
///
/// let arena = Arena::new();
/// let sub_arena = SubArena::new(&arena);
/// arena.alloc("Does not work!");
/// ```
pub struct SubArena<'a, T> {
    inner: Arena<T>,
    old: RefMut<'a, ChunkList<T>>,
    old_len: usize,
}

/// Hacky workaround to deal with variance issues
///
/// If `SubArena::new()` gives you lifetime issues, this might resolve them.
///
/// Ideally, one would be able to allocate shorter-lived things in a sub-arena:
///
/// ```compile_fail
/// use typed_arena::{Arena, SubArena};
///
/// let arena = Arena::<&'static i32>::new();
/// let x = arena.alloc(&5);
/// let y = arena.alloc(&2);
/// { // 'a
///     let a = 17;
///     let sub_arena = SubArena::new(&arena);
///     // The sub-arena should be for &'a i32, a subtype of &'static i32
///     // But Rust can't figure that out
///     let z = sub_arena.alloc(&a); // Doesn't work!
///     assert_eq!(*x + *y + *z, 24);
/// }
/// assert_eq!(*x + *y, 7);
/// ```
///
/// If it were possible to express subtypes as trait bounds, this could be
/// handled by `SubArena::new`.  However, this is currently impossible.
///
/// Instead, the subtyping can be handled by going through the intermediate
/// `SubArenaBuilder<T>` type, which is covariant in `T`:
///
/// ```
/// use typed_arena::{Arena, SubArenaBuilder};
///
/// let arena = Arena::<&'static i32>::new();
/// let x = arena.alloc(&5);
/// let y = arena.alloc(&2);
/// { // 'a
///     let a = 17;
///     let sub_arena = SubArenaBuilder::new(&arena).build();
///     // Rust is able to upcast the `SubArenaBuilder<&'static i32>` into a
///     // `SubArenaBuilder<&'a i32>` before calling `.build()` on it
///     let z = sub_arena.alloc(&a); // Works!
///     assert_eq!(*x + *y + *z, 24);
/// }
/// assert_eq!(*x + *y, 7);
/// ```
pub struct SubArenaBuilder<'a, T> {
    data: RefMut<'a, ()>,
    _marker: PhantomData<&'a ChunkList<T>>,
}

impl<'a, T> SubArenaBuilder<'a, T> {
    /// Create a new `SubArenaBuilder<T>`, on which the `.build()` method will create a
    /// `SubArena<T>`.
    ///
    /// See the `SubArenaBuilder` docs for why this is useful.
    pub fn new(arena: &'a Arena<T>) -> Self {
        unsafe {
            let data = RefMut::map(arena.chunks.borrow_mut(), |chunks| mem::transmute(chunks));
            Self {
                data,
                _marker: PhantomData,
            }
        }
    }

    /// Turn a `SubArenaBuilder<T>` into a `SubArena<T>`.
    ///
    /// See the `SubArenaBuilder` docs for why this is useful.
    pub fn build(self) -> SubArena<'a, T> {
        unsafe {
            let data = RefMut::map(self.data, |chunks| mem::transmute(chunks));
            SubArena::from_chunks(data)
        }
    }
}

impl<'a, T> SubArena<'a, T> {
    /// Create a new `SubArena`, with the given arena as its base
    ///
    /// If this method gives you lifetime errors, replacing `SubArena::new(&arena)`
    /// with `SubArenaBuilder::new(&arena).build()` might resolve them. See the
    /// `SubArenaBuilder` docs for an explanation.
    ///
    /// ## Example
    ///
    /// ```
    /// use typed_arena::Arena;
    /// use typed_arena::SubArena;
    ///
    /// let arena = Arena::new();
    /// let x = arena.alloc(1);
    /// let sub_one = SubArena::new(&arena);
    /// let y = sub_one.alloc(3);
    /// let sub_two = SubArena::new(&*sub_one);
    /// let z = sub_two.alloc(5);
    /// assert_eq!(*x + *y + *z, 9);
    /// // z's lifetime ends when sub_two is dropped
    /// drop(sub_two);
    /// assert_eq!(*x + *y, 4);
    /// // y's lifetime ends when sub_one is dropped
    /// drop(sub_one);
    /// assert_eq!(*x, 1);
    /// ```
    pub fn new(arena: &'a Arena<T>) -> Self {
        let old = arena.chunks.borrow_mut();
        Self::from_chunks(old)
    }

    fn from_chunks(mut old: RefMut<'a, ChunkList<T>>) -> Self {
        let inner_vec = mem::replace(&mut old.current, Vec::new());
        let old_len = inner_vec.len();
        let inner = Arena {
            chunks: RefCell::new(ChunkList {
                current: inner_vec,
                rest: Vec::new(),
            }),
        };
        Self {
            inner,
            old,
            old_len,
        }
    }
}

impl<'a, T> Drop for SubArena<'a, T> {
    fn drop(&mut self) {
        let inner = self.inner.chunks.get_mut();
        let mut stolen_vec = mem::replace(
            inner.rest.get_mut(0).unwrap_or(&mut inner.current),
            Vec::new(),
        );
        while stolen_vec.len() > self.old_len {
            stolen_vec.pop();
        }

        mem::swap(&mut stolen_vec, &mut self.old.current);
    }
}

impl<'a, T> Deref for SubArena<'a, T> {
    type Target = Arena<T>;

    fn deref(&self) -> &Arena<T> {
        &self.inner
    }
}
