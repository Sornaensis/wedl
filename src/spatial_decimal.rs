//! Exact nonnegative arithmetic over the YAML parser's canonical number text.
//! Values have at most 1000 coefficient/scale digits; no float addition occurs.
use crate::{Error, Result};
use serde_yaml::Number;
use std::{cmp::Ordering, fmt};

const MAX_DIGITS: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Decimal {
    digits: Vec<u8>,
    scale: usize,
}

fn overflow() -> Error {
    Error::InvalidInput("weighted path total exceeds the 1000-digit decimal budget".into())
}

impl Decimal {
    pub(super) fn zero() -> Self {
        Self {
            digits: vec![0],
            scale: 0,
        }
    }

    fn normalized(mut digits: Vec<u8>, mut scale: usize) -> Result<Self> {
        let Some(first) = digits.iter().position(|value| *value != 0) else {
            return Ok(Self::zero());
        };
        digits.drain(..first);
        while scale > 0 && digits.last() == Some(&0) {
            digits.pop();
            scale -= 1;
        }
        if digits.len() > MAX_DIGITS || scale > MAX_DIGITS {
            return Err(overflow());
        }
        Ok(Self { digits, scale })
    }

    pub(super) fn number(number: &Number) -> Result<Self> {
        let representation = number.to_string();
        let raw = representation.strip_prefix('-').unwrap_or(&representation);
        let (mantissa, exponent) = if let Some(index) = raw.find(['e', 'E']) {
            (
                &raw[..index],
                raw[index + 1..].parse::<i32>().map_err(|_| overflow())?,
            )
        } else {
            (raw, 0)
        };
        let fractional = mantissa.split_once('.').map_or(0, |(_, tail)| tail.len());
        let mut digits = Vec::new();
        for byte in mantissa.bytes().filter(|byte| *byte != b'.') {
            if !byte.is_ascii_digit() {
                return Err(Error::Validation(
                    "metric must have a finite decimal representation".into(),
                ));
            }
            digits.push(byte - b'0');
        }
        if digits.is_empty() {
            return Err(Error::Validation("metric has no decimal digits".into()));
        }
        if representation.starts_with('-') && digits.iter().any(|digit| *digit != 0) {
            return Err(Error::Validation("metric must be nonnegative".into()));
        }
        let scale = fractional as i32 - exponent;
        let scale = if scale < 0 {
            let padding = scale.unsigned_abs() as usize;
            if digits.len() + padding > MAX_DIGITS {
                return Err(overflow());
            }
            digits.resize(digits.len() + padding, 0);
            0
        } else {
            scale as usize
        };
        Self::normalized(digits, scale)
    }

    pub(super) fn add(&self, other: &Self) -> Result<Self> {
        let scale = self.scale.max(other.scale);
        let left_padding = scale - self.scale;
        let right_padding = scale - other.scale;
        let length = (self.digits.len() + left_padding).max(other.digits.len() + right_padding);
        if length > MAX_DIGITS {
            return Err(overflow());
        }
        let digit = |value: &Self, padding: usize, index: usize| {
            index
                .checked_sub(padding)
                .and_then(|index| value.digits.len().checked_sub(index + 1))
                .map_or(0, |index| value.digits[index])
        };
        let mut result = vec![0; length + 1];
        let mut carry = 0;
        for index in 0..length {
            let sum = digit(self, left_padding, index) + digit(other, right_padding, index) + carry;
            result[length - index] = sum % 10;
            carry = sum / 10;
        }
        result[0] = carry;
        Self::normalized(result, scale)
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        let left_zero = self.digits == [0];
        let right_zero = other.digits == [0];
        match (left_zero, right_zero) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        let left_width = self.digits.len() as isize - self.scale as isize;
        let right_width = other.digits.len() as isize - other.scale as isize;
        left_width.cmp(&right_width).then_with(|| {
            (0..self.digits.len().max(other.digits.len()))
                .map(|index| {
                    self.digits
                        .get(index)
                        .copied()
                        .unwrap_or(0)
                        .cmp(&other.digits.get(index).copied().unwrap_or(0))
                })
                .find(|comparison| *comparison != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        })
    }
}
impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl fmt::Display for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits: String = self
            .digits
            .iter()
            .map(|digit| char::from(b'0' + digit))
            .collect();
        if self.scale == 0 {
            return formatter.write_str(&digits);
        }
        if self.digits.len() > self.scale {
            let split = self.digits.len() - self.scale;
            write!(formatter, "{}.{}", &digits[..split], &digits[split..])
        } else {
            write!(
                formatter,
                "0.{}{}",
                "0".repeat(self.scale - self.digits.len()),
                digits
            )
        }
    }
}
