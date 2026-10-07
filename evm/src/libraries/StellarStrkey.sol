// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

/// @notice Strict on-chain validation of Stellar strkeys (G, C, M): base32
/// alphabet, canonical length and padding, CRC16-XModem checksum and version.
/// Mirrors libs/crosschain-types/src/strkey.rs.
library StellarStrkey {
    enum Kind {
        Account,
        Contract,
        Muxed
    }

    uint8 internal constant VERSION_ACCOUNT = 6 << 3;
    uint8 internal constant VERSION_CONTRACT = 2 << 3;
    uint8 internal constant VERSION_MUXED = 12 << 3;

    error InvalidStrkey();
    error UnsupportedStrkeyKind();

    function validate(string memory strkey) internal pure returns (Kind) {
        bytes memory s = bytes(strkey);
        if (s.length != 56 && s.length != 69) revert InvalidStrkey();
        bytes memory raw = new bytes(s.length * 5 / 8);
        uint256 buffer;
        uint256 bits;
        uint256 j;
        for (uint256 i = 0; i < s.length; i++) {
            buffer = (buffer << 5) | _value(s[i]);
            bits += 5;
            if (bits >= 8) {
                bits -= 8;
                raw[j++] = bytes1(uint8(buffer >> bits));
                buffer &= (1 << bits) - 1;
            }
        }
        if (bits >= 5 || buffer != 0 || j != raw.length) revert InvalidStrkey();

        uint256 n = raw.length;
        uint16 expected = _crc16(raw, n - 2);
        if (uint8(raw[n - 2]) != uint8(expected) || uint8(raw[n - 1]) != uint8(expected >> 8)) {
            revert InvalidStrkey();
        }
        uint8 version = uint8(raw[0]);
        if (version == VERSION_ACCOUNT && n == 35) return Kind.Account;
        if (version == VERSION_CONTRACT && n == 35) return Kind.Contract;
        if (version == VERSION_MUXED && n == 43) return Kind.Muxed;
        if (version == VERSION_ACCOUNT || version == VERSION_CONTRACT || version == VERSION_MUXED) {
            revert InvalidStrkey();
        }
        revert UnsupportedStrkeyKind();
    }

    function _value(bytes1 c) private pure returns (uint256) {
        uint8 x = uint8(c);
        if (x >= 0x41 && x <= 0x5A) return x - 0x41;
        if (x >= 0x32 && x <= 0x37) return x - 0x32 + 26;
        revert InvalidStrkey();
    }

    function _crc16(bytes memory data, uint256 len) private pure returns (uint16 crc) {
        for (uint256 i = 0; i < len; i++) {
            crc ^= uint16(uint8(data[i])) << 8;
            for (uint256 k = 0; k < 8; k++) {
                crc = crc & 0x8000 != 0 ? (crc << 1) ^ 0x1021 : crc << 1;
            }
        }
    }
}
