//! Cyclic redundancy checks: one implementation, parameterised the way the
//! CRC catalogue writes every CRC down (Williams's model: width,
//! polynomial, initial value, reflection in and out, final XOR), and each
//! CRC a protocol frames with a named constant of it.
//!
//! Until 2026-09-24 the IEEE 802.15.4 frame check sequence (Thread and
//! `WirelessHART`), DNP3's, EN 13757-4's, Kafka's CRC-32C and the RFCOMM
//! frame check sequence were five loops in five crates, each with its
//! polynomial written reflected by hand. A protocol that needs another CRC
//! adds a constant here with its catalogue check value, and a test holds it
//! to that value.
//!
//! The width is the register's: `u8`, `u16`, `u32` or `u64`. Bit-at-a-time,
//! because every frame these check is short.

use core::ops::{BitAnd, BitXor, Shl, Shr};

/// The register a CRC is computed in; its width is the CRC's width.
pub trait Register:
    Copy
    + Eq
    + BitAnd<Output = Self>
    + BitXor<Output = Self>
    + Shl<u32, Output = Self>
    + Shr<u32, Output = Self>
{
    /// The register's width in bits.
    const WIDTH: u32;
    /// All bits clear.
    const ZERO: Self;
    /// The lowest bit.
    const LOW: Self;
    /// The highest bit.
    const HIGH: Self;
    /// `byte` in the register's lowest eight bits.
    fn from_byte(byte: u8) -> Self;
    /// The bits in the opposite order.
    #[must_use]
    fn reflect(self) -> Self;
}

macro_rules! register {
    ($($width:ty),*) => {$(
        impl Register for $width {
            const WIDTH: u32 = <$width>::BITS;
            const ZERO: Self = 0;
            const LOW: Self = 1;
            const HIGH: Self = 1 << (<$width>::BITS - 1);
            fn from_byte(byte: u8) -> Self {
                Self::from(byte)
            }
            fn reflect(self) -> Self {
                self.reverse_bits()
            }
        }
    )*};
}

register!(u8, u16, u32, u64);

/// One CRC, as the catalogue parameterises it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Crc<R> {
    /// The generator polynomial, top bit implied, written unreflected.
    pub polynomial: R,
    /// The register before the first byte, written unreflected.
    pub initial: R,
    /// Whether each byte enters the register lowest bit first.
    pub reflect_in: bool,
    /// Whether the register is reflected before the final XOR.
    pub reflect_out: bool,
    /// What the register is combined with, by exclusive or, at the end.
    pub xor_out: R,
    /// The CRC of the nine ASCII bytes `123456789`, as the catalogue gives it.
    pub check: R,
}

impl<R: Register> Crc<R> {
    /// The CRC of `bytes`.
    #[must_use]
    pub fn checksum(&self, bytes: &[u8]) -> R {
        let register = if self.reflect_in {
            let polynomial = self.polynomial.reflect();
            bytes
                .iter()
                .fold(self.initial.reflect(), |mut register, byte| {
                    register = register ^ R::from_byte(*byte);
                    for _ in 0..8 {
                        register = if register & R::LOW == R::ZERO {
                            register >> 1
                        } else {
                            (register >> 1) ^ polynomial
                        };
                    }
                    register
                })
        } else {
            bytes.iter().fold(self.initial, |mut register, byte| {
                register = register ^ (R::from_byte(*byte) << (R::WIDTH - 8));
                for _ in 0..8 {
                    register = if register & R::HIGH == R::ZERO {
                        register << 1
                    } else {
                        (register << 1) ^ self.polynomial
                    };
                }
                register
            })
        };
        let register = if self.reflect_in == self.reflect_out {
            register
        } else {
            register.reflect()
        };
        register ^ self.xor_out
    }
}

/// CRC-16/KERMIT: the IEEE 802.15.4 frame check sequence, which Thread,
/// `WirelessHART` and every other technology on that radio close a frame
/// with, least significant byte first.
pub const CRC_16_KERMIT: Crc<u16> = Crc {
    polynomial: 0x1021,
    initial: 0,
    reflect_in: true,
    reflect_out: true,
    xor_out: 0,
    check: 0x2189,
};

/// CRC-16/DNP: the check IEEE 1815 (DNP3) puts after a link header and
/// after every sixteen bytes of user data.
pub const CRC_16_DNP: Crc<u16> = Crc {
    polynomial: 0x3d65,
    initial: 0,
    reflect_in: true,
    reflect_out: true,
    xor_out: 0xffff,
    check: 0xea82,
};

/// CRC-16/EN-13757: the check wireless M-Bus (EN 13757-4) puts after
/// every block of a frame.
pub const CRC_16_EN_13757: Crc<u16> = Crc {
    polynomial: 0x3d65,
    initial: 0,
    reflect_in: false,
    reflect_out: false,
    xor_out: 0xffff,
    check: 0xc2b7,
};

/// CRC-32/ISCSI, Castagnoli's CRC-32C: what a Kafka record batch header
/// carries over everything after it.
pub const CRC_32_ISCSI: Crc<u32> = Crc {
    polynomial: 0x1edc_6f41,
    initial: 0xffff_ffff,
    reflect_in: true,
    reflect_out: true,
    xor_out: 0xffff_ffff,
    check: 0xe306_9283,
};

/// The frame check sequence of 3GPP TS 27.010 (formerly GSM 07.10), which
/// Bluetooth RFCOMM frames with: CRC-8/ROHC's polynomial, initial value and
/// reflection, complemented at the end. The catalogue lists CRC-8/ROHC
/// (check `0xd0`); this is it with `xor_out` `0xff`, so its check is
/// `0xd0 ^ 0xff`.
pub const CRC_8_TS_27_010: Crc<u8> = Crc {
    polynomial: 0x07,
    initial: 0xff,
    reflect_in: true,
    reflect_out: true,
    xor_out: 0xff,
    check: 0x2f,
};

#[cfg(test)]
mod tests {
    use super::*;

    const CHECK: &[u8] = b"123456789";

    #[test]
    fn every_named_crc_gives_its_catalogued_check_value() {
        for crc in [CRC_16_KERMIT, CRC_16_DNP, CRC_16_EN_13757] {
            assert_eq!(crc.checksum(CHECK), crc.check, "{crc:?}");
        }
        assert_eq!(CRC_32_ISCSI.checksum(CHECK), CRC_32_ISCSI.check);
        assert_eq!(CRC_8_TS_27_010.checksum(CHECK), CRC_8_TS_27_010.check);
    }

    #[test]
    fn the_model_reproduces_catalogue_crcs_nobody_frames_with_yet() {
        // CRC-8/ROHC, CRC-16/ARC (reflected), CRC-16/XMODEM (not),
        // CRC-32/ISO-HDLC and CRC-64/XZ, each with its catalogue check.
        let rohc = Crc {
            xor_out: 0,
            check: 0xd0,
            ..CRC_8_TS_27_010
        };
        assert_eq!(rohc.checksum(CHECK), rohc.check);
        let arc = Crc {
            polynomial: 0x8005,
            check: 0xbb3d,
            ..CRC_16_KERMIT
        };
        assert_eq!(arc.checksum(CHECK), arc.check);
        let xmodem = Crc {
            polynomial: 0x1021,
            xor_out: 0,
            check: 0x31c3,
            ..CRC_16_EN_13757
        };
        assert_eq!(xmodem.checksum(CHECK), xmodem.check);
        let iso_hdlc = Crc {
            polynomial: 0x04c1_1db7,
            check: 0xcbf4_3926,
            ..CRC_32_ISCSI
        };
        assert_eq!(iso_hdlc.checksum(CHECK), iso_hdlc.check);
        let xz: Crc<u64> = Crc {
            polynomial: 0x42f0_e1eb_a9ea_3693,
            initial: u64::MAX,
            reflect_in: true,
            reflect_out: true,
            xor_out: u64::MAX,
            check: 0x995d_c9bb_df19_39fa,
        };
        assert_eq!(xz.checksum(CHECK), xz.check);
    }

    #[test]
    fn reflection_out_alone_reflects_the_result() {
        // No CRC named here reflects out without reflecting in, so this
        // holds that branch to its definition.
        let out_only = Crc {
            reflect_out: true,
            ..CRC_16_EN_13757
        };
        let plain = CRC_16_EN_13757.checksum(CHECK) ^ CRC_16_EN_13757.xor_out;
        assert_eq!(out_only.checksum(CHECK), plain.reverse_bits() ^ 0xffff);
    }

    #[test]
    fn nothing_checks_to_the_initial_value_through_the_final_xor() {
        assert_eq!(CRC_16_KERMIT.checksum(b""), 0);
        assert_eq!(CRC_16_DNP.checksum(b""), 0xffff);
        assert_eq!(CRC_32_ISCSI.checksum(b""), 0);
    }
}
