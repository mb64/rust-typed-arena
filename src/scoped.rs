use crate::Arena;
use crate::ChunkList;

use core::cell::{RefCell, RefMut};
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

impl<'a, T> SubArena<'a, T> {
    /// Create a new `SubArena`, with the given arena as its base
    ///
    /// Note that since `SubArena`s implement `Deref<Target=Arena>`, one can
    /// make a sub-arena from another sub-arena:
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
        let mut old = arena.chunks.borrow_mut();
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
