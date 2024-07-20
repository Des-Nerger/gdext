/*
 * Copyright (c) godot-rust; Bromeon and contributors.
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use crate::builtin::Variant;
use crate::meta::error::ConvertError;
use crate::meta::{ClassName, FromGodot, GodotConvert, GodotFfiVariant, GodotType, ToGodot};
use crate::obj::{bounds, Bounds, Gd, GodotClass, Inherits, RawGd};
use crate::sys;
use godot_ffi::{GodotFfi, GodotNullableFfi, PtrcallType};
use std::ptr;

/// Objects that can be passed as arguments to Godot engine functions.
///
/// This trait is implemented for the following types:
/// - [`Gd<T>`] and `&Gd<T>`, to pass objects. Subclasses of `T` are explicitly supported.
/// - [`Option<Gd<T>>`] and `Option<&Gd<T>>`, to pass optional objects. `None` is mapped to a null argument.
/// - [`NullArg`], to pass `null` arguments without using `Option`.
///
/// # Nullability
/// <div class="warning">
/// The GDExtension API does not inform about nullability of its function parameters. It is up to you to verify that the arguments you pass
/// are only null when this is allowed. Doing this wrong should be safe, but can lead to the function call failing.
/// </div>
pub trait AsObjectArg<T>
where
    T: GodotClass + Bounds<Declarer = bounds::DeclEngine>,
{
    #[doc(hidden)]
    fn as_object_arg(&self) -> ObjectArg<T>;
}

impl<T, U> AsObjectArg<T> for Gd<U>
where
    T: GodotClass + Bounds<Declarer = bounds::DeclEngine>,
    U: Inherits<T>,
{
    fn as_object_arg(&self) -> ObjectArg<T> {
        <&Gd<U>>::as_object_arg(&self)
    }
}

impl<T, U> AsObjectArg<T> for &Gd<U>
where
    T: GodotClass + Bounds<Declarer = bounds::DeclEngine>,
    U: Inherits<T>,
{
    fn as_object_arg(&self) -> ObjectArg<T> {
        ObjectArg::from_raw_gd(&self.raw)
    }
}

impl<T, U> AsObjectArg<T> for Option<U>
where
    T: GodotClass + Bounds<Declarer = bounds::DeclEngine>,
    U: AsObjectArg<T>,
{
    fn as_object_arg(&self) -> ObjectArg<T> {
        self.as_ref()
            .map_or_else(ObjectArg::null, AsObjectArg::as_object_arg)
    }
}

impl<T> AsObjectArg<T> for NullArg
where
    T: GodotClass + Bounds<Declarer = bounds::DeclEngine>,
{
    fn as_object_arg(&self) -> ObjectArg<T> {
        ObjectArg::null()
    }
}

// ----------------------------------------------------------------------------------------------------------------------------------------------

/// Represents `null` when passing an object argument to Godot.
///
/// This can be used whenever a Godot signature accepts [`AsObjectArg<T>`].
/// Using `NullArg` is equivalent to passing `Option::<Gd<T>>::None`, but less wordy.
///
/// This expression is only intended for function argument lists. To work with objects that can be null, use `Option<Gd<T>>` instead.
///
/// For APIs that accept `Variant`, you can pass [`Variant::nil()`] instead.
///
/// # Nullability
/// <div class="warning">
/// The GDExtension API does not inform about nullability of its function parameters. It is up to you to verify that the arguments you pass
/// are only null when this is allowed. Doing this wrong should be safe, but can lead to the function call failing.
/// </div>
///
/// # Example
/// ```no_run
/// # fn some_shape() -> Gd<GltfPhysicsShape> { unimplemented!() }
/// use godot::prelude::*;
/// use godot_core::classes::GltfPhysicsShape;
///
/// let mut shape: Gd<GltfPhysicsShape> = some_shape();
/// shape.set_importer_mesh(NullArg);
pub struct NullArg;

// ----------------------------------------------------------------------------------------------------------------------------------------------

/// View for object arguments passed to the Godot engine. Never owning; must be null or backed by `Gd<T>`.
///
/// Could technically have a lifetime, but this makes the whole calling code more complex, e.g. `type CallSig`. Since usage is quite localized
/// and this type doesn't use `Drop` or is propagated further, this should be fine.
#[derive(Debug)]
#[doc(hidden)]
pub struct ObjectArg<T: GodotClass> {
    // Never dropped since it's just a view; see constructor.
    object_ptr: sys::GDExtensionObjectPtr,
    _marker: std::marker::PhantomData<*mut T>,
}

impl<T> ObjectArg<T>
where
    T: GodotClass,
{
    pub fn from_raw_gd<Derived>(obj: &RawGd<Derived>) -> Self
    where
        Derived: Inherits<T>,
    {
        // Runtime check is necessary, to ensure that object is still alive and has correct runtime type.
        if !obj.is_null() {
            obj.check_rtti("from_raw_gd");
        }

        Self {
            object_ptr: obj.obj_sys(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn null() -> Self {
        Self {
            object_ptr: ptr::null_mut(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn is_null(&self) -> bool {
        self.object_ptr.is_null()
    }
}

// #[derive(Clone)] doesn't seem to get bounds right.
impl<T: GodotClass> Clone for ObjectArg<T> {
    fn clone(&self) -> Self {
        Self {
            object_ptr: self.object_ptr,
            _marker: std::marker::PhantomData,
        }
    }
}

// SAFETY: see impl GodotFfi for RawGd.
unsafe impl<T> GodotFfi for ObjectArg<T>
where
    T: GodotClass,
{
    // If anything changes here, keep in sync with RawGd impl.

    fn variant_type() -> sys::VariantType {
        sys::VariantType::OBJECT
    }

    unsafe fn new_from_sys(_ptr: sys::GDExtensionConstTypePtr) -> Self {
        unreachable!("ObjectArg should only be passed *to* Godot, not *from*.")
    }

    unsafe fn new_with_uninit(_init: impl FnOnce(sys::GDExtensionUninitializedTypePtr)) -> Self {
        unreachable!("ObjectArg should only be passed *to* Godot, not *from*.")
    }

    unsafe fn new_with_init(_init: impl FnOnce(sys::GDExtensionTypePtr)) -> Self {
        unreachable!("ObjectArg should only be passed *to* Godot, not *from*.")
    }

    fn sys(&self) -> sys::GDExtensionConstTypePtr {
        self.object_ptr.cast()
    }

    fn sys_mut(&mut self) -> sys::GDExtensionTypePtr {
        self.object_ptr.cast()
    }

    // For more context around `ref_get_object` and `ref_set_object`, see:
    // https://github.com/godotengine/godot-cpp/issues/954

    fn as_arg_ptr(&self) -> sys::GDExtensionConstTypePtr {
        // See RawGd::as_arg_ptr().
        #[cfg(before_api = "4.1")]
        {
            self.sys()
        }

        #[cfg(since_api = "4.1")]
        {
            ptr::addr_of!(self.object_ptr) as sys::GDExtensionConstTypePtr
        }
    }

    unsafe fn from_arg_ptr(_ptr: sys::GDExtensionTypePtr, _call_type: PtrcallType) -> Self {
        unreachable!("ObjectArg should only be passed *to* Godot, not *from*.")
    }

    unsafe fn move_return_ptr(self, _ptr: sys::GDExtensionTypePtr, _call_type: PtrcallType) {
        unreachable!("ObjectArg should only be passed *to* Godot, not *from*.")
    }
}

impl<T: GodotClass> GodotConvert for ObjectArg<T> {
    type Via = Self;
}

impl<T: GodotClass> ToGodot for ObjectArg<T> {
    fn to_godot(&self) -> Self::Via {
        (*self).clone()
    }

    fn into_godot(self) -> Self::Via {
        self
    }
}

impl<T: GodotClass> FromGodot for ObjectArg<T> {
    fn try_from_godot(_via: Self::Via) -> Result<Self, ConvertError> {
        unreachable!("ObjectArg should only be passed *to* Godot, not *from*.")
    }
}

impl<T: GodotClass> GodotType for ObjectArg<T> {
    type Ffi = Self;

    fn to_ffi(&self) -> Self::Ffi {
        (*self).clone()
    }

    fn into_ffi(self) -> Self::Ffi {
        self
    }

    fn try_from_ffi(_ffi: Self::Ffi) -> Result<Self, ConvertError> {
        //unreachable!("ObjectArg should only be passed *to* Godot, not *from*.")
        Ok(_ffi)
    }

    fn class_name() -> ClassName {
        T::class_name()
    }

    fn godot_type_name() -> String {
        T::class_name().to_string()
    }
}

impl<T: GodotClass> GodotFfiVariant for ObjectArg<T> {
    fn ffi_to_variant(&self) -> Variant {
        unreachable!("ObjectArg::ffi_to_variant() is not expected to be called.")
    }

    fn ffi_from_variant(_variant: &Variant) -> Result<Self, ConvertError> {
        unreachable!("ObjectArg should only be passed *to* Godot, not *from*.")
    }
}

impl<T: GodotClass> GodotNullableFfi for ObjectArg<T> {
    fn flatten_option(opt: Option<Self>) -> Self {
        opt.unwrap_or_else(Self::null)
    }

    fn is_null(&self) -> bool {
        Self::is_null(self)
    }
}
