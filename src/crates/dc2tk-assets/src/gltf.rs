//! glTF's accessor types, which the export (to_gltf) writes and the import (from_gltf) reads: the
//! component type of each number and the element type of each value (glTF 2.0, section 3.6.2).

use crate::numeric::{self, Num};

/// An accessor's componentType: what each number is stored as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComponentType {
    Byte,
    UnsignedByte,
    Short,
    UnsignedShort,
    UnsignedInt,
    Float,
}

impl ComponentType {
    /// The type of a componentType's number (5120 to 5126), or None for one glTF does not have.
    pub(crate) fn from_code(code: i64) -> Option<ComponentType> {
        Some(match code {
            5120 => ComponentType::Byte,
            5121 => ComponentType::UnsignedByte,
            5122 => ComponentType::Short,
            5123 => ComponentType::UnsignedShort,
            5125 => ComponentType::UnsignedInt,
            5126 => ComponentType::Float,
            _ => return None,
        })
    }

    /// The componentType's number, as the JSON writes it.
    pub(crate) fn code(self) -> i64 {
        match self {
            ComponentType::Byte => 5120,
            ComponentType::UnsignedByte => 5121,
            ComponentType::Short => 5122,
            ComponentType::UnsignedShort => 5123,
            ComponentType::UnsignedInt => 5125,
            ComponentType::Float => 5126,
        }
    }

    /// The bytes a number takes.
    pub(crate) fn size(self) -> usize {
        match self {
            ComponentType::Byte | ComponentType::UnsignedByte => 1,
            ComponentType::Short | ComponentType::UnsignedShort => 2,
            ComponentType::UnsignedInt | ComponentType::Float => 4,
        }
    }

    /// What a normalised number is divided by to give -1 to 1 (or 0 to 1); None for the types a
    /// normalised accessor cannot have.
    pub(crate) fn divisor(self) -> Option<f64> {
        match self {
            ComponentType::Byte => Some(127.0),
            ComponentType::UnsignedByte => Some(255.0),
            ComponentType::Short => Some(32767.0),
            ComponentType::UnsignedShort => Some(65535.0),
            ComponentType::UnsignedInt | ComponentType::Float => None,
        }
    }

    /// Whether a sparse accessor's indices may be of this type (an unsigned integer).
    pub(crate) fn is_unsigned(self) -> bool {
        matches!(self, ComponentType::UnsignedByte | ComponentType::UnsignedShort | ComponentType::UnsignedInt)
    }

    /// The number little-endian `bytes` (`size()` of them) hold: an int, or a float as a double.
    pub(crate) fn read(self, bytes: &[u8]) -> Num {
        let word = || u32::from_le_bytes(bytes[..4].try_into().expect("a 4-byte component"));
        match self {
            ComponentType::Byte => Num::Int(bytes[0] as i8 as i64),
            ComponentType::UnsignedByte => Num::Int(bytes[0] as i64),
            ComponentType::Short => Num::Int(i16::from_le_bytes([bytes[0], bytes[1]]) as i64),
            ComponentType::UnsignedShort => Num::Int(u16::from_le_bytes([bytes[0], bytes[1]]) as i64),
            ComponentType::UnsignedInt => Num::Int(word() as i64),
            ComponentType::Float => Num::Float(numeric::f32_value(word())),
        }
    }
}

/// An accessor's type: how many numbers make each of its values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccessorType {
    Scalar,
    Vec2,
    Vec3,
    Vec4,
    Mat4,
}

impl AccessorType {
    /// The type of a type's name ("VEC3"), or None for one glTF does not have.
    pub(crate) fn from_name(name: &str) -> Option<AccessorType> {
        Some(match name {
            "SCALAR" => AccessorType::Scalar,
            "VEC2" => AccessorType::Vec2,
            "VEC3" => AccessorType::Vec3,
            "VEC4" => AccessorType::Vec4,
            "MAT4" => AccessorType::Mat4,
            _ => return None,
        })
    }

    /// The type's name, as the JSON writes it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            AccessorType::Scalar => "SCALAR",
            AccessorType::Vec2 => "VEC2",
            AccessorType::Vec3 => "VEC3",
            AccessorType::Vec4 => "VEC4",
            AccessorType::Mat4 => "MAT4",
        }
    }

    /// The numbers of each value.
    pub(crate) fn components(self) -> usize {
        match self {
            AccessorType::Scalar => 1,
            AccessorType::Vec2 => 2,
            AccessorType::Vec3 => 3,
            AccessorType::Vec4 => 4,
            AccessorType::Mat4 => 16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_read_their_codes_and_names_back() {
        for code in [5120, 5121, 5122, 5123, 5125, 5126] {
            assert_eq!(ComponentType::from_code(code).unwrap().code(), code);
        }
        assert_eq!(ComponentType::from_code(5124), None);
        for name in ["SCALAR", "VEC2", "VEC3", "VEC4", "MAT4"] {
            assert_eq!(AccessorType::from_name(name).unwrap().name(), name);
        }
        assert_eq!(AccessorType::from_name("MAT3"), None);
        assert!(matches!(ComponentType::Short.read(&[0xFE, 0xFF]), Num::Int(-2)));
        assert!(matches!(ComponentType::Float.read(&1.5f32.to_le_bytes()), Num::Float(x) if x == 1.5));
        assert_eq!((ComponentType::UnsignedByte.divisor(), ComponentType::Float.divisor()), (Some(255.0), None));
    }
}
