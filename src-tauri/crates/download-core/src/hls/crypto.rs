use crate::CoreError;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// Déchiffre des données AES-128-CBC (padding PKCS7).
pub fn decrypt_aes128_cbc(data: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Result<Vec<u8>, CoreError> {
    Aes128CbcDec::new(key.into(), iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .map_err(|e| CoreError::Media(crate::trf!("échec déchiffrement AES-128: {e}", "AES-128 decryption failed: {e}")))
}

/// IV d'un segment : IV explicite s'il existe, sinon le numéro de séquence média
/// codé en big-endian sur 128 bits.
pub fn iv_for_segment(explicit: Option<[u8; 16]>, media_sequence: u64) -> [u8; 16] {
    if let Some(iv) = explicit {
        return iv;
    }
    let mut iv = [0u8; 16];
    iv[8..16].copy_from_slice(&media_sequence.to_be_bytes());
    iv
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

    #[test]
    fn decrypt_roundtrips_encrypt() {
        let key = [0x42u8; 16];
        let iv = [0x01u8; 16];
        let plaintext = b"hello hls segment payload!!".to_vec();
        let ct = Aes128CbcEnc::new((&key).into(), (&iv).into())
            .encrypt_padded_vec_mut::<Pkcs7>(&plaintext);
        let pt = decrypt_aes128_cbc(&ct, &key, &iv).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn iv_explicit_is_returned() {
        let iv = [0xABu8; 16];
        assert_eq!(iv_for_segment(Some(iv), 5), iv);
    }

    #[test]
    fn iv_from_sequence_is_big_endian() {
        let iv = iv_for_segment(None, 1);
        let mut expected = [0u8; 16];
        expected[15] = 1;
        assert_eq!(iv, expected);
    }
}
