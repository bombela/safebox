#![allow(unused_variables)]
#![allow(dead_code)]

//! The SafeBox lowers the risk of leaving copies of its content linger in RAM.
//!
//! To zero a piece RAM requires a bit more than calling memset(0). There is few layers of
//! optimizations and abstractions working against us:
//!
//!  1) The compiler is free to forgo zeroing altogether. Since the value is never used again, the
//!     compiler assumes no side-effect, and will happily remove the code in the name of speed.
//!  2) The compiler is free to reorder memory operations.
//!  3) Similarly, the hardware is free to reorder memory operations.
//!  4) Furthermore, the hardware might not flush caches to RAM right away.
//!
//! With the above taken care of, we must also enforce that the content cannot be copied in RAM
//! inadvertently:
//!
//!  5) Taking ownership of the content can move it anywhere, including from/to the stack/heap.
//!     Leaving a stray copy of the content in RAM behind.
//!  6) Similarly, a &mut access on the content allows a mem swap or mem replace.
//!
//! And finally, the Operating System can be involved:
//!
//!  7) The Operating System can move the RAM to the SWAP area on persistent storage.
//!  8) Any thread in the same memory space can read & write the memory at will.
//!  9) When hibernating the OS will likely copy the RAM content to persistent storage.
//!
//! This crate solves 1) to 4) with volatile write and atomic fence.
//! 5) and 6) are guarded behind unsafe functions. Of course, the programmer is responsible to
//!    maintain the invariant; but at last; it is requires using a visible unsafe block.
//!
//! The Operating System side is ignored. 7) and 8) could be addressed via mlock and mprotect
//! syscalls. And as for 9), you should use an encrypted storage anyway.
//!
//!
//! Example:
//! ```
//! use safebox::SafeBox;
//! use rand::prelude::*;
//! let mut secret = SafeBox::new_slice(0_u8, 8);
//! unsafe {
//!     thread_rng().fill(secret.get_mut());
//! }
//! unsafe {
//!     println!("My secret: {:?}", secret.get_ref());
//! }
//! ```
//!
//! Prints (non-deterministic):
//! ```text
//! My secret: [242, 144, 235, 196, 84, 35, 85, 232]
//! ```
//!
//! See [`SafeBox::new_slice_with`] for a more elegant random initialization.

use std::mem;
use std::sync::atomic;

/// Set the memory behind a value to zero.
///
/// The value pointed by T will be replaced by zeroes in RAM. This is guaranteed to not be
/// optimized away by the compiler and hardware.
///
/// This is unsafe, because T is left in some uninitialized state. It is easy to get into Undefined
/// Behavior territory with this.
///
pub unsafe fn memzero<T: ?Sized>(p: &mut T) {
    // If T is !Sized, returns the pointed value size in bytes.
    // If T is Sized, returns the size of T in bytes.
    let len: usize = mem::size_of_val(p);

    // TODO replace loop by volatile_set_memory whenever it is stabilized.
    let raw: *mut u8 = (p as *mut T).cast();
    for i in 0..len {
        // write_volatile is guaranteed to not be elided nor reordered.
        raw.add(i).write_volatile(0_u8);
    }
    // Smarter people than me said this flushes memory IO to RAM. I believe them.
    atomic::fence(atomic::Ordering::SeqCst);
}

/// A safe box for your secrets.
///
/// On Drop the content T is zeroed in RAM with [`memzero`].
///
/// It can only be instantiated with Copy types. This forbids instantiating a `SafeBox<Vec<T>>` for
/// example, which cannot be zeroed.
///
/// &T access is guarded behind the `unsafe get_ref` method. This prevents involuntary copies or
/// clone of the content. Deref is not implemented.
///
/// &mut T is also guarded behind `unsafe get_mut`. This prevents involuntary memswap or memreplace
/// of the content. And because DerefMut is not implemented, the content cannot be moved out either.
/// Remember that it is perfectly safe to move or swap the SafeBox itself, because the content never
/// moves, merely the smart pointer details.
///
/// Because only Copy types are accepted for the content, it is possible to provide a safe
/// implementation of Clone. It allocates a new SafeBox with a memcopy of the content.
///
/// It is implemented as a wrapper around a Box<T>.
pub struct SafeBox<T: ?Sized>(Box<T>);

impl<T: ?Sized> Drop for SafeBox<T> {
    fn drop(&mut self) {
        unsafe {
            memzero(&mut self.0 as &mut T);
        }
        // We only construct from T: Copy, which implies T: !Drop.
        // Therefor the content of the Box cannot have any destructor to run.
    }
}

impl<T: Copy> SafeBox<T> {
    /// Allocate a new SafeBox from the given value.
    ///
    /// Since v is passed by copy/move, it is advised to initialize with some safe value. Then use
    /// [`SafeBox::get_mut`] to write the secret value with the least amount of intermediate
    /// copies.
    pub fn new(v: T) -> Self {
        Self(Box::new(v))
    }
}

impl<T: Default + Copy> Default for SafeBox<T> {
    /// Allocate a new SafeBox with the default value.
    ///
    /// See [`SafeBox::new`].
    fn default() -> Self {
        SafeBox::<T>::new(T::default())
    }
}

impl<T: Copy> SafeBox<[T]> {
    /// Allocate a new `SafeBox<[T]>`.
    ///
    /// The value `v` is copied into all `len` elements.
    pub fn new_slice(v: T, len: usize) -> Self {
        Self(vec![v; len].into_boxed_slice())
    }
}

impl<T> SafeBox<[T]> {
    /// Allocate a new `SafeBox<[T]>`.
    ///
    /// The function `f` is called to initialize the `len` elements.
    ///
    /// ```
    /// use safebox::SafeBox;
    /// use rand::prelude::*;
    /// let random_secret = SafeBox::new_slice_with(8, &random::<u8>);
    /// ```
    pub fn new_slice_with<F: Fn() -> T>(len: usize, f: F) -> Self {
        Self(
            std::iter::repeat_with(f)
                .take(len)
                .collect::<Vec<T>>()
                .into_boxed_slice(),
        )
    }
}

impl<T: ?Sized> SafeBox<T> {
    /// A `&T` reference to the content.
    ///
    /// This is unsafe, because it allows for copying the content around in memory. Of course, a
    /// secret must be read at some point to be useful. But you bear all responsibility in copying
    /// it around.
    pub unsafe fn get_ref(&self) -> &T {
        &self.0
    }

    /// A `&mut T` reference to the content.
    ///
    /// This is unsafe, because it allows for copying the content around in memory. Of course, a
    /// secret must be initialized at some point to be useful. But you bear all responsibility in
    /// copying it around.
    pub unsafe fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Copy> Clone for SafeBox<T> {
    /// Clone a `SafeBox<T>` via memcopy.
    fn clone(&self) -> Self {
        // Box::new(*self.0) could copy on the stack. Hence the ptr dance.
        let mut clone = SafeBox::new(mem::MaybeUninit::<T>::uninit());
        let dest_ptr = clone.0.as_mut_ptr();
        unsafe {
            dest_ptr.copy_from_nonoverlapping(&*self.0 as *const T, 1);
            // MaybeUninit is guaranteed to have the same memory layout as its content.
            mem::transmute(clone)
        }
    }
}

impl<T: Copy> Clone for SafeBox<[T]> {
    /// Clone a `SafeBox<[T]>` via memcopy.
    fn clone(&self) -> Self {
        let len = self.0.len();
        let clone = SafeBox::new_slice(mem::MaybeUninit::<T>::uninit(), len);
        unsafe {
            // MaybeUninit is guaranteed to have the same memory layout as its content.
            let mut clone: SafeBox<[T]> = mem::transmute(clone);

            let dest_ptr = clone.0.as_mut_ptr();
            dest_ptr.copy_from_nonoverlapping(self.0.as_ptr(), len);
            clone
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Debug, Clone)]
    struct Foo {
        v: i32,
    }

    impl Drop for Foo {
        fn drop(&mut self) {
            println!("drop {:?}", self);
        }
    }

    #[test]
    fn scalar() {
        let s: SafeBox<u32> = SafeBox::new(42_u32);
        assert_eq!(unsafe { s.get_ref() }, &42_u32);
        let p: *const u32 = unsafe { s.get_ref() };
        drop(s);
        // p is a dangling pointer now. Another test running concurrently might have reallocated
        // the piece of RAM already. Let's play anyway.
        assert_eq!(unsafe { p.read_volatile() }, 0_u32);
    }

    #[test]
    fn slice() {
        let reference: &mut [i32] = &mut vec![42; 100];
        let mut s: SafeBox<[i32]> = SafeBox::new_slice(42, 100);
        assert_eq!(unsafe { s.get_ref() }, reference);
        unsafe {
            s.get_mut()[78] = 99;
        };
        reference[78] = 99;
        assert_eq!(unsafe { s.get_ref() }, reference);

        let reference: &[i32] = &vec![0; 100];
        let p: *const [i32] = unsafe { s.get_ref() };
        drop(s);
        // p is dandling, its dangerous, etc. you know the story.
        assert_eq!(unsafe { &*p }, reference);
    }

    #[test]
    fn random_secret() {
        use rand::prelude::*;
        let random_secret = SafeBox::new_slice_with(8, &random::<u8>);
        unsafe {
            println!("My secret: {:?}", random_secret.get_ref());
        }
    }

    #[test]
    fn clone() {
        use rand::prelude::*;
        let a = SafeBox::new_slice_with(256, &random::<i32>);
        let mut b = a.clone();
        unsafe {
            assert_eq!(a.get_ref(), b.get_ref());
        }
        drop(a);
        let a = SafeBox::new_slice_with(256, &random::<i32>);
        unsafe {
            assert_ne!(a.get_ref(), b.get_ref());
        }
        b = a.clone();
        unsafe {
            assert_eq!(a.get_ref(), b.get_ref());
        }
    }
}
