//! A dead simple "bump allocator" Store.
//!
//! A store which references a stack or statically allocated fixed-sized block of memory. Multiple instances may
//! reference the same block, and all instances referencing the same block are fungible.

use core::{
    alloc::{AllocError, Layout},
    cell::{Cell, UnsafeCell},
    fmt,
    marker::PhantomData,
    mem::{self, MaybeUninit},
    ptr::{self, Alignment, NonNull},
};

use crate::interface::{Store, StoreDangling, StoreMultiple, StorePinning, StoreSharing, StoreStable};

/// The backing block of memory for the store.
///
/// Generic parameters:
///
/// -   The block of memory is aligned and sized as per `T`.
pub struct StackBumpBlock<T> {
    watermark: Cell<usize>,
    memory: UnsafeCell<MaybeUninit<T>>,
}

impl<T> StackBumpBlock<T> {
    /// Creates a new, empty, block.
    pub fn new() -> Self {
        let watermark = Cell::new(0);
        let memory = UnsafeCell::new(MaybeUninit::uninit());

        Self { watermark, memory }
    }

    /// Creates a new store referencing this block.
    pub fn create_store<H>(&self) -> StackBumpStore<'_, H> {
        let watermark = &self.watermark;

        let memory = {
            let length = mem::size_of::<T>();
            let address = NonNull::from(&self.memory).cast();

            NonNull::slice_from_raw_parts(address, length)
        };

        let _marker = PhantomData;

        StackBumpStore {
            watermark,
            memory,
            _marker,
        }
    }
}

impl<T> Default for StackBumpBlock<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A store instance referencing its block.
///
/// Generic parameters:
///
/// -   `H` is the handle type, it must convertible to and from `usize`.
pub struct StackBumpStore<'a, H> {
    watermark: &'a Cell<usize>,
    memory: NonNull<[u8]>,
    _marker: PhantomData<fn(H) -> H>,
}

//  Cannot be const, because TryFrom is not marked #[const_trait].
unsafe impl<'a, H> StoreDangling for StackBumpStore<'a, H>
where
    H: Copy + TryFrom<usize>,
{
    type Handle = H;

    fn dangling(&self, alignment: Alignment) -> Result<Self::Handle, AllocError> {
        Self::from_offset(alignment.as_usize())
    }
}

unsafe impl<'a, H> Store for StackBumpStore<'a, H>
where
    H: Copy + TryFrom<usize> + TryInto<usize>,
{
    fn allocate(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        let (result, new_watermark) = self.compute_offset(layout)?;
        self.watermark.set(new_watermark);

        Ok((result, layout.size()))
    }

    #[inline(always)]
    unsafe fn deallocate(&self, _handle: Self::Handle, _layout: Layout) {}

    #[inline(always)]
    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8> {
        debug_assert!(Self::into_offset(handle) <= self.memory.len());

        let offset = Self::into_offset(handle);
        let pointer = self.memory.as_mut_ptr();

        //  Safety:
        //  -   `offset` is within bounds of `self.memory`, as `handle` was allocated by `self` as per pre-conditions.
        let pointer = unsafe { pointer.add(offset) };

        //  Safety:
        //  -   `pointer` is non null as `self` is non null.
        unsafe { NonNull::new_unchecked(pointer) }
    }

    unsafe fn grow(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "{new_layout:?} must have a greater size than {old_layout:?}"
        );

        //  As an optimization, if `handle` points to the last allocation, growth may actually occur _in place_.
        {
            let offset = Self::into_offset(handle);
            let watermark = self.watermark.get();

            if offset + old_layout.size() == watermark
                && new_layout.align() <= old_layout.align()
                && offset + new_layout.size() <= self.memory.len()
            {
                let new_watermark = watermark - old_layout.size() + new_layout.size();
                self.watermark.set(new_watermark);

                return Ok((handle, new_layout.size()));
            }
        }

        self.grow_by_relocation(handle, old_layout, new_layout)
    }

    #[inline(always)]
    unsafe fn shrink(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        _new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            _new_layout.size() >= old_layout.size(),
            "{_new_layout:?} must have a smaller size than {old_layout:?}"
        );

        Ok((handle, old_layout.size()))
    }
}

//  Safety:
//  -   Handles remain valid across all operations on `self`.
unsafe impl<'a, H> StoreMultiple for StackBumpStore<'a, H> where H: Copy + TryFrom<usize> + TryInto<usize> {}

//  Safety:
//  -   `self.resolve(handle)` always returns the same address.
unsafe impl<'a, H> StoreStable for StackBumpStore<'a, H> where H: Copy + TryFrom<usize> + TryInto<usize> {}

//  Safety:
//  -   `self.resolve(handle)` always returns the same address.
unsafe impl<'a, H> StorePinning for StackBumpStore<'a, H> where H: Copy + TryFrom<usize> + TryInto<usize> {}

/// Safety:
/// -   All instances referencing the same StackBumpBlock are fungible.
unsafe impl<'a, H> StoreSharing for StackBumpStore<'a, H>
where
    H: Copy + TryFrom<usize> + TryInto<usize>,
{
    type SharingError = !;

    fn is_sharing_with(&self, other: &Self) -> bool {
        self.memory == other.memory
    }

    fn share(&self) -> Result<Self, Self::SharingError>
    where
        Self: Sized,
    {
        let watermark = self.watermark;
        let memory = self.memory;
        let _marker = PhantomData;

        Ok(Self {
            watermark,
            memory,
            _marker,
        })
    }
}

impl<'a, H> fmt::Debug for StackBumpStore<'a, H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.debug_struct("StackBumpStore")
            .field("watermark", &self.watermark)
            .field("memory", &self.memory.len())
            .finish()
    }
}

//
//  Implementation
//

impl<'a, H> StackBumpStore<'a, H>
where
    H: TryFrom<usize>,
{
    #[inline(always)]
    fn from_offset(offset: usize) -> Result<H, AllocError> {
        offset.try_into().map_err(|_| AllocError)
    }
}

impl<'a, H> StackBumpStore<'a, H>
where
    H: TryInto<usize>,
{
    #[inline(always)]
    fn into_offset(handle: H) -> usize {
        let offset = handle.try_into();

        debug_assert!(offset.is_ok());

        //  Safety:
        //  -   `handle` was created from `usize`, hence converting back always succeeds.
        unsafe { offset.unwrap_unchecked() }
    }
}

impl<'a, H> StackBumpStore<'a, H>
where
    H: TryFrom<usize> + TryInto<usize>,
{
    //  Returns the offset and new watermark of the newly allocated memory block.
    fn compute_offset(&self, layout: Layout) -> Result<(H, usize), AllocError> {
        let watermark = self.watermark.get();

        let aligned = {
            //  Since `layout.align()` is always a power of 2, aligning to the next multiple of `layout.align()` can be
            //  done with this one simple trick.
            let alignment_mask = layout.align() - 1;

            (watermark + alignment_mask) & !alignment_mask
        };

        let new_watermark = aligned + layout.size();

        if new_watermark > self.memory.len() {
            return Err(AllocError);
        }

        let aligned = Self::from_offset(aligned)?;

        Ok((aligned, new_watermark))
    }
}

impl<'a, H> StackBumpStore<'a, H>
where
    H: Copy + TryFrom<usize> + TryInto<usize>,
{
    //  Slow part of `grow`.
    #[inline(never)]
    fn grow_by_relocation(&self, handle: H, old_layout: Layout, new_layout: Layout) -> Result<(H, usize), AllocError> {
        let (result, new_watermark) = self.compute_offset(new_layout)?;
        self.watermark.set(new_watermark);

        //  Safety:
        //  -   `handle` is valid, as per pre-conditions.
        //  -   `result` is valid, since newly allocated.
        let (new, old) = unsafe { (self.resolve(result), self.resolve(handle)) };

        //  Safety:
        //  -   `old` is valid for `old_layout.size()` bytes, as per pre-conditions.
        //  -   `new` is valid for `old_layout.size()` bytes, since it is valid for `new_layout.size()` bytes and as per
        //      pre-conditions `new_layout.size() >= old_layout.size()`.
        //  -   `old` and `new` are at least 1-byte aligned.
        //  -   `old` and `new` point to non-overlapping areas, since `old` points to a memory area prior to the
        //      watermark and `new` points to a memory area post the watermark (as the beginning of this function),
        //      since `old_layout` fits `old` as per pre-conditions.
        unsafe { ptr::copy_nonoverlapping(old.as_ptr(), new.as_ptr(), old_layout.size()) };

        Ok((result, new_layout.size()))
    }
}
