//! NaN-boxed value representation for the Forge VM.
//!
//! Encodes all VM values in a single 64-bit word using IEEE 754 quiet NaN
//! payload bits. This halves memory usage compared to the 16-byte enum.
//!
//! ## Encoding scheme
//!
//! ```text
//! Float:  any f64 that does NOT match our quiet NaN tag pattern
//!         (normal numbers, infinities, and canonical NaN are all valid floats)
//!
//! Tagged: bits [63..0]
//!   [1][11111111111][1][TTT][PPPPPPPP...48 bits...PPPP]
//!    ^   exponent    ^  tag         payload
//!    sign=1       quiet=1
//!
//! Tag (bits 48-50):
//!   000 = Null      payload ignored (canonical: 0)
//!   001 = Bool      payload = 0 (false) or 1 (true)
//!   010 = Int       payload = 48-bit signed integer (two's complement)
//!   011 = Obj       payload = 48-bit GcRef index
//! ```
//!
//! We use sign=1 + quiet NaN to create a tag space that doesn't collide with
//! any real f64 value. The canonical NaN (0x7FF8000000000000) has sign=0,
//! so it's recognized as a float, not a tagged value.

use super::gc::Gc;
use super::value::GcRef;
use std::fmt;

/// Quiet NaN with sign bit set — our tag prefix.
/// Bits: 1_11111111111_1_000_000...0 = 0xFFF8_0000_0000_0000
const NANBOX_PREFIX: u64 = 0xFFF8_0000_0000_0000;

/// Mask for the prefix bits (sign + exponent + quiet bit).
const PREFIX_MASK: u64 = 0xFFF8_0000_0000_0000;

/// Tag bits occupy bits 48-50 (3 bits).
const TAG_SHIFT: u32 = 48;
const TAG_MASK: u64 = 0x0007_0000_0000_0000; // bits 48-50

/// Payload mask: lower 48 bits.
const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Maximum positive integer that fits in 48-bit signed two's complement.
const INT48_MAX: i64 = (1_i64 << 47) - 1; // 140,737,488,355,327
/// Minimum negative integer that fits in 48-bit signed two's complement.
const INT48_MIN: i64 = -(1_i64 << 47); // -140,737,488,355,328

// Tag values
const TAG_NULL: u64 = 0;
const TAG_BOOL: u64 = 1;
const TAG_INT: u64 = 2;
const TAG_OBJ: u64 = 3;

/// A NaN-boxed VM value packed into 8 bytes.
#[derive(Clone, Copy)]
pub struct NanBoxedValue(u64);

impl NanBoxedValue {
    // ---- Constructors ----

    /// Box a float value. Canonicalizes NaN to ensure it doesn't collide with tags.
    #[inline]
    pub fn from_float(f: f64) -> Self {
        let bits = f.to_bits();
        // If the float happens to look like our tagged prefix, canonicalize it.
        // This can only happen for NaN values (all exponent bits set + nonzero mantissa).
        if bits & PREFIX_MASK == NANBOX_PREFIX {
            // Replace with canonical NaN (sign=0, quiet=1, payload=0)
            return NanBoxedValue(0x7FF8_0000_0000_0000);
        }
        NanBoxedValue(bits)
    }

    /// Box a signed integer. Values outside 48-bit range return None (caller must box on heap).
    #[inline]
    pub fn try_from_int(n: i64) -> Option<Self> {
        if n >= INT48_MIN && n <= INT48_MAX {
            // Store as 48-bit two's complement
            let payload = (n as u64) & PAYLOAD_MASK;
            Some(NanBoxedValue(
                NANBOX_PREFIX | (TAG_INT << TAG_SHIFT) | payload,
            ))
        } else {
            None // Caller should allocate ObjKind::BoxedInt
        }
    }

    /// Box an integer, panicking if it doesn't fit in 48 bits.
    /// Use only when the value is known to be small (e.g., array length, bool-to-int).
    #[inline]
    pub fn from_small_int(n: i64) -> Self {
        Self::try_from_int(n).expect("BUG: integer too large for inline NaN-boxing")
    }

    /// Box a boolean.
    #[inline]
    pub fn from_bool(b: bool) -> Self {
        let payload = b as u64;
        NanBoxedValue(NANBOX_PREFIX | (TAG_BOOL << TAG_SHIFT) | payload)
    }

    /// The null value.
    #[inline]
    pub fn null() -> Self {
        NanBoxedValue(NANBOX_PREFIX | (TAG_NULL << TAG_SHIFT))
    }

    /// Box a GC reference.
    #[inline]
    pub fn from_obj(r: GcRef) -> Self {
        let idx = r.0 as u64;
        debug_assert!(
            idx <= PAYLOAD_MASK,
            "GcRef index exceeds 48-bit NaN-box payload"
        );
        NanBoxedValue(NANBOX_PREFIX | (TAG_OBJ << TAG_SHIFT) | (idx & PAYLOAD_MASK))
    }

    // ---- Type queries ----

    /// Is this a float (any f64 that isn't our tagged NaN pattern)?
    #[inline]
    pub fn is_float(&self) -> bool {
        self.0 & PREFIX_MASK != NANBOX_PREFIX
    }

    /// Is this a tagged value (not a float)?
    #[inline]
    fn is_tagged(&self) -> bool {
        self.0 & PREFIX_MASK == NANBOX_PREFIX
    }

    /// Get the tag bits (only valid when `is_tagged()` is true).
    #[inline]
    fn tag(&self) -> u64 {
        (self.0 & TAG_MASK) >> TAG_SHIFT
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.is_tagged() && self.tag() == TAG_NULL
    }

    #[inline]
    pub fn is_bool(&self) -> bool {
        self.is_tagged() && self.tag() == TAG_BOOL
    }

    #[inline]
    pub fn is_int(&self) -> bool {
        self.is_tagged() && self.tag() == TAG_INT
    }

    #[inline]
    pub fn is_obj(&self) -> bool {
        self.is_tagged() && self.tag() == TAG_OBJ
    }

    // ---- Extractors ----

    /// Extract a float. Returns None if this is a tagged value.
    #[inline]
    pub fn as_float(&self) -> Option<f64> {
        if self.is_float() {
            Some(f64::from_bits(self.0))
        } else {
            None
        }
    }

    /// Extract a signed integer. Returns None if not an int.
    #[inline]
    pub fn as_int(&self) -> Option<i64> {
        if self.is_int() {
            let raw = self.0 & PAYLOAD_MASK;
            // Sign-extend from bit 47
            let shifted = (raw as i64) << 16;
            Some(shifted >> 16)
        } else {
            None
        }
    }

    /// Extract a boolean. Returns None if not a bool.
    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        if self.is_bool() {
            Some((self.0 & 1) != 0)
        } else {
            None
        }
    }

    /// Extract a GC reference. Returns None if not an obj.
    #[inline]
    pub fn as_obj(&self) -> Option<GcRef> {
        if self.is_obj() {
            Some(GcRef((self.0 & PAYLOAD_MASK) as usize))
        } else {
            None
        }
    }

    /// Get the raw u64 bits (for serialization or debugging).
    #[inline]
    pub fn to_bits(&self) -> u64 {
        self.0
    }

    /// Construct from raw bits (for deserialization).
    #[inline]
    pub fn from_bits(bits: u64) -> Self {
        NanBoxedValue(bits)
    }

    // ---- Value methods (matching existing Value API) ----

    pub fn is_truthy(&self, gc: &Gc) -> bool {
        if let Some(b) = self.as_bool() {
            return b;
        }
        if let Some(n) = self.as_int() {
            return n != 0;
        }
        if let Some(f) = self.as_float() {
            return f != 0.0;
        }
        if self.is_null() {
            return false;
        }
        if let Some(r) = self.as_obj() {
            return gc.get(r).is_some_and(|obj| match &obj.kind {
                super::value::ObjKind::String(s) => !s.is_empty(),
                super::value::ObjKind::Array(a) => !a.is_empty(),
                super::value::ObjKind::Object(o) => !o.is_empty(),
                super::value::ObjKind::ResultOk(_) => true,
                super::value::ObjKind::ResultErr(_) => false,
                _ => true,
            });
        }
        false
    }

    pub fn type_name(&self, gc: &Gc) -> &'static str {
        if self.is_int() {
            "Int"
        } else if self.is_float() {
            "Float"
        } else if self.is_bool() {
            "Bool"
        } else if self.is_null() {
            "Null"
        } else if let Some(r) = self.as_obj() {
            gc.get(r).map_or("Null", |o| o.type_name())
        } else {
            "Unknown"
        }
    }

    pub fn display(&self, gc: &Gc) -> String {
        if let Some(n) = self.as_int() {
            n.to_string()
        } else if let Some(f) = self.as_float() {
            format!("{}", f)
        } else if let Some(b) = self.as_bool() {
            b.to_string()
        } else if self.is_null() {
            "null".to_string()
        } else if let Some(r) = self.as_obj() {
            gc.get(r).map_or("<freed>".to_string(), |o| o.display(gc))
        } else {
            "<unknown>".to_string()
        }
    }

    pub fn to_json_string(&self, gc: &Gc) -> String {
        if let Some(n) = self.as_int() {
            n.to_string()
        } else if let Some(f) = self.as_float() {
            format!("{}", f)
        } else if let Some(b) = self.as_bool() {
            b.to_string()
        } else if self.is_null() {
            "null".to_string()
        } else if let Some(r) = self.as_obj() {
            gc.get(r)
                .map_or("null".to_string(), |o| o.to_json_string(gc))
        } else {
            "null".to_string()
        }
    }

    pub fn equals(&self, other: &NanBoxedValue, gc: &Gc) -> bool {
        // Fast path: identical bit patterns (but NOT for floats due to NaN != NaN, -0 == +0)
        if self.is_tagged() && other.is_tagged() && self.0 == other.0 {
            return true;
        }

        // Float comparisons
        if let (Some(a), Some(b)) = (self.as_float(), other.as_float()) {
            return a == b; // IEEE 754 semantics: NaN != NaN, -0 == +0
        }

        // Int-Float cross-comparison
        if let (Some(a), Some(b)) = (self.as_int(), other.as_float()) {
            return (a as f64) == b;
        }
        if let (Some(a), Some(b)) = (self.as_float(), other.as_int()) {
            return a == (b as f64);
        }

        // Int == Int already handled by bit pattern equality above
        // Bool == Bool already handled by bit pattern equality above
        // Null == Null already handled by bit pattern equality above

        // Obj == Obj: structural equality
        if let (Some(a), Some(b)) = (self.as_obj(), other.as_obj()) {
            if a == b {
                return true;
            }
            return match (gc.get(a), gc.get(b)) {
                (Some(oa), Some(ob)) => oa.equals(ob, gc),
                _ => false,
            };
        }

        false
    }

    /// Check structural identity for constant dedup (no GC needed).
    #[allow(dead_code)]
    pub fn identical(&self, other: &NanBoxedValue) -> bool {
        if let (Some(a), Some(b)) = (self.as_int(), other.as_int()) {
            return a == b;
        }
        if let (Some(a), Some(b)) = (self.as_float(), other.as_float()) {
            return a == b;
        }
        if let (Some(a), Some(b)) = (self.as_bool(), other.as_bool()) {
            return a == b;
        }
        if self.is_null() && other.is_null() {
            return true;
        }
        false
    }
}

impl fmt::Debug for NanBoxedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = self.as_int() {
            write!(f, "{}", n)
        } else if let Some(fl) = self.as_float() {
            write!(f, "{}", fl)
        } else if let Some(b) = self.as_bool() {
            write!(f, "{}", b)
        } else if self.is_null() {
            write!(f, "null")
        } else if let Some(r) = self.as_obj() {
            write!(f, "Obj({})", r.0)
        } else {
            write!(f, "NanBox(0x{:016x})", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Float round-trips ----

    #[test]
    fn float_zero() {
        let v = NanBoxedValue::from_float(0.0);
        assert!(v.is_float());
        assert!(!v.is_int());
        assert!(!v.is_null());
        assert_eq!(v.as_float(), Some(0.0));
    }

    #[test]
    fn float_negative_zero() {
        let v = NanBoxedValue::from_float(-0.0);
        assert!(v.is_float());
        assert_eq!(v.as_float(), Some(-0.0));
        // -0.0 and 0.0 are equal per IEEE 754
        assert_eq!(v.as_float().unwrap(), 0.0);
    }

    #[test]
    fn float_positive() {
        let v = NanBoxedValue::from_float(3.14);
        assert!(v.is_float());
        assert_eq!(v.as_float(), Some(3.14));
    }

    #[test]
    fn float_negative() {
        let v = NanBoxedValue::from_float(-42.5);
        assert!(v.is_float());
        assert_eq!(v.as_float(), Some(-42.5));
    }

    #[test]
    fn float_infinity() {
        let v = NanBoxedValue::from_float(f64::INFINITY);
        assert!(v.is_float());
        assert_eq!(v.as_float(), Some(f64::INFINITY));
    }

    #[test]
    fn float_neg_infinity() {
        let v = NanBoxedValue::from_float(f64::NEG_INFINITY);
        assert!(v.is_float());
        assert_eq!(v.as_float(), Some(f64::NEG_INFINITY));
    }

    #[test]
    fn float_canonical_nan() {
        let v = NanBoxedValue::from_float(f64::NAN);
        assert!(v.is_float());
        // Should be recognized as float, not tagged
        let f = v.as_float().unwrap();
        assert!(f.is_nan());
    }

    #[test]
    fn float_max() {
        let v = NanBoxedValue::from_float(f64::MAX);
        assert!(v.is_float());
        assert_eq!(v.as_float(), Some(f64::MAX));
    }

    #[test]
    fn float_min_positive() {
        let v = NanBoxedValue::from_float(f64::MIN_POSITIVE);
        assert!(v.is_float());
        assert_eq!(v.as_float(), Some(f64::MIN_POSITIVE));
    }

    #[test]
    fn float_subnormal() {
        let v = NanBoxedValue::from_float(5e-324);
        assert!(v.is_float());
        assert_eq!(v.as_float(), Some(5e-324));
    }

    // ---- Integer round-trips ----

    #[test]
    fn int_zero() {
        let v = NanBoxedValue::from_small_int(0);
        assert!(v.is_int());
        assert!(!v.is_float());
        assert_eq!(v.as_int(), Some(0));
    }

    #[test]
    fn int_positive() {
        let v = NanBoxedValue::from_small_int(42);
        assert!(v.is_int());
        assert_eq!(v.as_int(), Some(42));
    }

    #[test]
    fn int_negative() {
        let v = NanBoxedValue::from_small_int(-1);
        assert!(v.is_int());
        assert_eq!(v.as_int(), Some(-1));
    }

    #[test]
    fn int_negative_large() {
        let v = NanBoxedValue::from_small_int(-12345);
        assert!(v.is_int());
        assert_eq!(v.as_int(), Some(-12345));
    }

    #[test]
    fn int_48bit_max() {
        let v = NanBoxedValue::try_from_int(INT48_MAX).unwrap();
        assert!(v.is_int());
        assert_eq!(v.as_int(), Some(INT48_MAX));
    }

    #[test]
    fn int_48bit_min() {
        let v = NanBoxedValue::try_from_int(INT48_MIN).unwrap();
        assert!(v.is_int());
        assert_eq!(v.as_int(), Some(INT48_MIN));
    }

    #[test]
    fn int_overflow_positive() {
        assert!(NanBoxedValue::try_from_int(INT48_MAX + 1).is_none());
    }

    #[test]
    fn int_overflow_negative() {
        assert!(NanBoxedValue::try_from_int(INT48_MIN - 1).is_none());
    }

    #[test]
    fn int_i64_max_overflows() {
        assert!(NanBoxedValue::try_from_int(i64::MAX).is_none());
    }

    #[test]
    fn int_i64_min_overflows() {
        assert!(NanBoxedValue::try_from_int(i64::MIN).is_none());
    }

    // ---- Bool round-trips ----

    #[test]
    fn bool_true() {
        let v = NanBoxedValue::from_bool(true);
        assert!(v.is_bool());
        assert!(!v.is_int());
        assert_eq!(v.as_bool(), Some(true));
    }

    #[test]
    fn bool_false() {
        let v = NanBoxedValue::from_bool(false);
        assert!(v.is_bool());
        assert_eq!(v.as_bool(), Some(false));
    }

    // ---- Null ----

    #[test]
    fn null() {
        let v = NanBoxedValue::null();
        assert!(v.is_null());
        assert!(!v.is_int());
        assert!(!v.is_float());
        assert!(!v.is_bool());
        assert!(!v.is_obj());
    }

    // ---- Obj round-trips ----

    #[test]
    fn obj_zero() {
        let v = NanBoxedValue::from_obj(GcRef(0));
        assert!(v.is_obj());
        assert_eq!(v.as_obj(), Some(GcRef(0)));
    }

    #[test]
    fn obj_large_index() {
        let v = NanBoxedValue::from_obj(GcRef(1_000_000));
        assert!(v.is_obj());
        assert_eq!(v.as_obj(), Some(GcRef(1_000_000)));
    }

    #[test]
    fn obj_max_48bit() {
        let max_idx = PAYLOAD_MASK as usize;
        let v = NanBoxedValue::from_obj(GcRef(max_idx));
        assert!(v.is_obj());
        assert_eq!(v.as_obj(), Some(GcRef(max_idx)));
    }

    // ---- Mutual exclusion ----

    #[test]
    fn types_are_mutually_exclusive() {
        let values = [
            NanBoxedValue::from_float(1.0),
            NanBoxedValue::from_small_int(1),
            NanBoxedValue::from_bool(true),
            NanBoxedValue::null(),
            NanBoxedValue::from_obj(GcRef(1)),
        ];
        for (i, v) in values.iter().enumerate() {
            let checks = [
                v.is_float(),
                v.is_int(),
                v.is_bool(),
                v.is_null(),
                v.is_obj(),
            ];
            let true_count: usize = checks.iter().filter(|&&c| c).count();
            assert_eq!(
                true_count, 1,
                "value at index {} has {} type flags set: {:?}",
                i, true_count, checks
            );
        }
    }

    // ---- Equality ----

    #[test]
    fn equals_int_int() {
        let gc = Gc::new();
        let a = NanBoxedValue::from_small_int(42);
        let b = NanBoxedValue::from_small_int(42);
        assert!(a.equals(&b, &gc));
    }

    #[test]
    fn equals_int_float_cross() {
        let gc = Gc::new();
        let a = NanBoxedValue::from_small_int(42);
        let b = NanBoxedValue::from_float(42.0);
        assert!(a.equals(&b, &gc));
        assert!(b.equals(&a, &gc));
    }

    #[test]
    fn equals_float_nan_not_equal() {
        let gc = Gc::new();
        let a = NanBoxedValue::from_float(f64::NAN);
        let b = NanBoxedValue::from_float(f64::NAN);
        assert!(!a.equals(&b, &gc));
    }

    #[test]
    fn equals_float_neg_zero() {
        let gc = Gc::new();
        let a = NanBoxedValue::from_float(0.0);
        let b = NanBoxedValue::from_float(-0.0);
        assert!(a.equals(&b, &gc));
    }

    #[test]
    fn not_equals_different_types() {
        let gc = Gc::new();
        let int_val = NanBoxedValue::from_small_int(1);
        let bool_val = NanBoxedValue::from_bool(true);
        assert!(!int_val.equals(&bool_val, &gc));
    }

    #[test]
    fn equals_null_null() {
        let gc = Gc::new();
        assert!(NanBoxedValue::null().equals(&NanBoxedValue::null(), &gc));
    }

    // ---- Debug formatting ----

    #[test]
    fn debug_format() {
        assert_eq!(format!("{:?}", NanBoxedValue::from_small_int(42)), "42");
        assert_eq!(format!("{:?}", NanBoxedValue::from_float(3.14)), "3.14");
        assert_eq!(format!("{:?}", NanBoxedValue::from_bool(true)), "true");
        assert_eq!(format!("{:?}", NanBoxedValue::null()), "null");
        assert_eq!(format!("{:?}", NanBoxedValue::from_obj(GcRef(7))), "Obj(7)");
    }

    // ---- Sign extension edge cases ----

    #[test]
    fn int_sign_extension_negative_one() {
        let v = NanBoxedValue::from_small_int(-1);
        assert_eq!(v.as_int(), Some(-1));
    }

    #[test]
    fn int_sign_extension_minus_max() {
        let v = NanBoxedValue::try_from_int(-INT48_MAX).unwrap();
        assert_eq!(v.as_int(), Some(-INT48_MAX));
    }

    #[test]
    fn int_boundary_values() {
        for n in [0, 1, -1, 100, -100, 1000000, -1000000, INT48_MAX, INT48_MIN] {
            let v = NanBoxedValue::try_from_int(n).unwrap();
            assert_eq!(v.as_int(), Some(n), "round-trip failed for {}", n);
        }
    }

    // ---- NaN canonicalization ----

    #[test]
    fn tagged_nan_is_canonicalized() {
        // Manually construct a float whose bits match our prefix
        // This would be a signaling NaN with sign=1
        let evil_bits: u64 = NANBOX_PREFIX | 0x0000_0000_0000_0042;
        let evil_float = f64::from_bits(evil_bits);
        let v = NanBoxedValue::from_float(evil_float);
        // Must be recognized as float (canonical NaN), not as a tagged value
        assert!(v.is_float());
        assert!(v.as_float().unwrap().is_nan());
        // Must not be confused with an int
        assert!(!v.is_int());
        assert!(!v.is_obj());
    }

    // ---- Identical (constant dedup) ----

    #[test]
    fn identical_ints() {
        let a = NanBoxedValue::from_small_int(5);
        let b = NanBoxedValue::from_small_int(5);
        assert!(a.identical(&b));
    }

    #[test]
    fn identical_different_types() {
        let a = NanBoxedValue::from_small_int(1);
        let b = NanBoxedValue::from_float(1.0);
        assert!(!a.identical(&b));
    }
}
