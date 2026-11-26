//! Native field types for dense homogeneous data.

macro_rules! packed {
    ($kind:ident, $element:ty, $description:literal) => {
        #[doc = $description]
        #[derive(Debug, Default, Clone, PartialEq)]
        #[repr(transparent)]
        pub struct $kind(pub Vec<$element>);

        impl From<Vec<$element>> for $kind {
            fn from(values: Vec<$element>) -> Self {
                Self(values)
            }
        }

        impl AsRef<[$element]> for $kind {
            fn as_ref(&self) -> &[$element] {
                &self.0
            }
        }
    };
}

packed!(PackedU16, u16, "A dense block of unsigned 16-bit integers.");
packed!(PackedU32, u32, "A dense block of unsigned 32-bit integers.");
packed!(PackedI32, i32, "A dense block of signed 32-bit integers.");
packed!(
    PackedF32,
    f32,
    "A dense block checked for finite floats at codec boundaries."
);
packed!(Bytes, u8, "A dense byte payload.");
