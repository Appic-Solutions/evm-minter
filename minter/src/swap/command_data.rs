use crate::rpc_declarations::Data;

// New function to decode commands_data from Vec<String> (hex strings) to Vec<Data>
pub fn decode_commands_data(commands_data: &[String]) -> Result<Vec<Data>, String> {
    commands_data
        .iter()
        .map(|hex_string| {
            let hex_str = hex_string
                .strip_prefix("0x")
                .ok_or_else(|| "Missing 0x prefix".to_string())?;
            let bytes = hex::decode(hex_str).map_err(|e| e.to_string())?;
            Ok(Data(bytes))
        })
        .collect()
}

// New function to encode Vec<Data> back to Vec<String> (hex strings with "0x")
pub fn encode_commands_data(data: &[Data]) -> Vec<String> {
    data.iter()
        .map(|d| format!("0x{}", hex::encode(&d.0)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_valid_commands_data() {
        let hexes = vec![
            "0xdeadbeef".to_string(),
            "0x1234567890abcdef".to_string(),
            "0x".to_string(), // empty
        ];
        let expected = vec![
            Data(vec![0xde, 0xad, 0xbe, 0xef]),
            Data(vec![0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef]),
            Data(vec![]),
        ];
        let decoded = decode_commands_data(&hexes).unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_decode_invalid_prefix() {
        let hexes = vec!["deadbeef".to_string()];
        let result = decode_commands_data(&hexes);
        assert!(result.is_err());
        if let Err(msg) = result {
            assert_eq!(msg, "Missing 0x prefix".to_string());
        } else {
            panic!("Unexpected error");
        }
    }

    #[test]
    fn test_decode_invalid_hex() {
        let hexes = vec!["0xgg".to_string()];
        let result = decode_commands_data(&hexes);
        assert!(result.is_err());
        if result.is_err() {
            // Error message from hex crate, but we check it's the right variant
        } else {
            panic!("Unexpected error");
        }
    }

    #[test]
    fn test_encode_commands_data() {
        let data = vec![
            Data(vec![0xde, 0xad, 0xbe, 0xef]),
            Data(vec![0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef]),
            Data(vec![]),
        ];
        let expected = vec![
            "0xdeadbeef".to_string(),
            "0x1234567890abcdef".to_string(),
            "0x".to_string(),
        ];
        let encoded = encode_commands_data(&data);
        assert_eq!(encoded, expected);
    }

    #[test]
    fn test_round_trip() {
        let original_hexes = vec![
            "0xdeadbeef".to_string(),
            "0x1234567890abcdef".to_string(),
            "0x".to_string(),
        ];
        let decoded = decode_commands_data(&original_hexes).unwrap();
        let encoded = encode_commands_data(&decoded);
        assert_eq!(original_hexes, encoded);

        // Also test the other way: start with Data, encode, decode
        let original_data = vec![
            Data(vec![0xde, 0xad, 0xbe, 0xef]),
            Data(vec![0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef]),
            Data(vec![]),
        ];
        let encoded = encode_commands_data(&original_data);
        let decoded = decode_commands_data(&encoded).unwrap();
        assert_eq!(original_data, decoded);
    }
}
