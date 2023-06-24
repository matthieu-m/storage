//! A dead simple "bump allocator" Store.
//!
//! This store is suitable for most containers -- such as `Box`, `BTreeMap`, `HashMap`, `List`, and `Vec` -- although in
//! some cases not all operations on the container may be enabled, for example `List::split` and `List::append` will not
//! be available.

use core::{
    alloc::{AllocError, Layout},
    cell::{Cell, UnsafeCell},
    fmt,
    mem::MaybeUninit,
    ptr::{self, Alignment, NonNull},
};

use crate::interface::{MultipleStore, StableStore, Store};

/// An implementation of `Store` providing a single, inline, block of memory.
///
/// Generic parameters:
///
/// -   `H` is the handle type, it must convertible to and from `usize`.
/// -   The block of memory is aligned and sized as per `T`.
pub struct InlineBumpStore<H, T> {
    watermark: Cell<H>,
    memory: UnsafeCell<MaybeUninit<T>>,
}

impl<H, T> InlineBumpStore<H, T>
where
    H: TryFrom<usize>,
{
    fn new() -> Result<Self, AllocError> {
        let _ = Self::from_offset(Self::memory_layout().size())?;

        let watermark = Cell::new(Self::from_offset(0)?);
        let memory = UnsafeCell::new(MaybeUninit::uninit());

        Ok(Self { watermark, memory })
    }
}

impl<H, T> Default for InlineBumpStore<H, T>
where
    H: TryFrom<usize>,
{
    fn default() -> Self {
        Self::new().expect("Size of `T` to be representable by `H`")
    }
}

unsafe impl<H, T> Store for InlineBumpStore<H, T>
where
    H: Copy + TryFrom<usize> + TryInto<usize>,
{
    type Handle = H;

    fn dangling(&self, alignment: Alignment) -> Result<Self::Handle, AllocError> {
        let layout = Self::memory_layout();

        if alignment.as_usize() > layout.align() {
            return Err(AllocError);
        }

        Self::from_offset(alignment.as_usize())
    }

    fn allocate(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        let (result, new_watermark) = Self::compute_offset(self.watermark.get(), layout)?;
        self.watermark.set(new_watermark);

        Ok((result, layout.size()))
    }

    #[inline(always)]
    unsafe fn deallocate(&self, _handle: Self::Handle, _layout: Layout) {}

    #[inline(always)]
    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8> {
        debug_assert!(Self::into_offset(handle) <= Self::memory_layout().size());

        let offset = Self::into_offset(handle);
        let pointer = self.memory.get() as *mut u8;

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
            let watermark = Self::into_offset(self.watermark.get());

            if offset + old_layout.size() == watermark
                && new_layout.align() <= old_layout.align()
                && offset + new_layout.size() <= Self::memory_layout().size()
            {
                let new_watermark = Self::from_offset(watermark - old_layout.size() + new_layout.size())?;
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
unsafe impl<H, T> MultipleStore for InlineBumpStore<H, T> where H: Copy + TryFrom<usize> + TryInto<usize> {}

//  Safety:
//  -   `self.resolve(handle)` always returns the same address, as long as `self` doesn't move.
unsafe impl<H, T> StableStore for InlineBumpStore<H, T> where H: Copy + TryFrom<usize> + TryInto<usize> {}

impl<H, T> fmt::Debug for InlineBumpStore<H, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let layout = Layout::new::<T>();

        f.debug_struct("InlineBumpStore")
            .field("size", &layout.size())
            .field("align", &layout.align())
            .finish()
    }
}

//
//  Implementation
//

impl<H, T> InlineBumpStore<H, T> {
    #[inline(always)]
    const fn memory_layout() -> Layout {
        Layout::new::<T>()
    }
}

impl<H, T> InlineBumpStore<H, T>
where
    H: TryFrom<usize>,
{
    #[inline(always)]
    fn from_offset(offset: usize) -> Result<H, AllocError> {
        debug_assert!(offset <= Self::memory_layout().size());

        offset.try_into().map_err(|_| AllocError)
    }
}

impl<H, T> InlineBumpStore<H, T>
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

impl<H, T> InlineBumpStore<H, T>
where
    H: TryFrom<usize> + TryInto<usize>,
{
    //  Returns the offset and new watermark of the newly allocated memory block.
    fn compute_offset(watermark: H, layout: Layout) -> Result<(H, H), AllocError> {
        let watermark = Self::into_offset(watermark);
        let memory = Self::memory_layout();

        if layout.align() > memory.align() {
            //  Even if the memory block was aligned for the current address of `self.memory`, moving `self` would risk
            //  breaking this alignment.

            return Err(AllocError);
        }

        let aligned = {
            //  Since `layout.align()` is always a power of 2, aligning to the next multiple of `layout.align()` can be
            //  done with this one simple trick.
            let alignment_mask = layout.align() - 1;

            (watermark + alignment_mask) & !alignment_mask
        };

        let new_watermark = aligned + layout.size();

        if new_watermark > memory.size() {
            return Err(AllocError);
        }

        let aligned = Self::from_offset(aligned)?;
        let new_watermark = Self::from_offset(new_watermark)?;

        Ok((aligned, new_watermark))
    }
}

impl<H, T> InlineBumpStore<H, T>
where
    H: Copy + TryFrom<usize> + TryInto<usize>,
{
    //  Slow part of `grow`.
    #[inline(never)]
    fn grow_by_relocation(&self, handle: H, old_layout: Layout, new_layout: Layout) -> Result<(H, usize), AllocError> {
        let (result, new_watermark) = Self::compute_offset(self.watermark.get(), new_layout)?;
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
