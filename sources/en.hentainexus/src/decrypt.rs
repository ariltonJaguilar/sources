//! Port of HentaiNexus's Utils.kt decryption algorithm.
//!
//! The page reader encodes a JSON array inside `initReader("<base64>", ...)`.
//! Decryption steps:
//!   1. Base64-decode the payload.
//!   2. XOR the first 15 bytes with the hostname bytes ("hentainexus.com").
//!   3. Use bytes 0..64 as an RC4-style key stream to build a substitution box.
//!   4. Derive a prime index via a CRC-like fold over the key stream.
//!   5. Decrypt bytes 64.. using a modified RC4 cipher keyed on that prime.
use aidoku::{
	Result,
	alloc::{String, Vec},
	prelude::*,
};
use base64::{Engine, engine::general_purpose::STANDARD};

const HOSTNAME: &[u8] = b"hentainexus.com";
const PRIME_NUMBERS: [u32; 8] = [2, 3, 5, 7, 11, 13, 17, 19];
const PRIME_IDX_XOR_MASK: u32 = 12;

pub fn decrypt_pages(encoded: &str) -> Result<String> {
	let mut data = STANDARD
		.decode(encoded)
		.map_err(|_| error!("base64 decode failed"))?;

	if data.len() < 65 {
		bail!("encrypted payload too short");
	}

	// Step 2 – XOR first 15 bytes with hostname
	for (i, &h) in HOSTNAME.iter().enumerate() {
		data[i] ^= h;
	}

	// Split into key stream (first 64 bytes) and ciphertext (rest)
	let key_stream: Vec<u32> = data[..64].iter().map(|&b| b as u32).collect();
	let ciphertext: Vec<u32> = data[64..].iter().map(|&b| b as u32).collect();

	// Step 3 – build substitution box using RC4 KSA
	let mut digest: Vec<u32> = (0u32..=255).collect();
	let mut key: u32 = 0;
	for i in 0..=255usize {
		key = (key + digest[i] + key_stream[i % 64]) % 256;
		digest.swap(i, key as usize);
	}

	// Step 4 – derive prime index via CRC-like fold
	let mut prime_idx: u32 = 0;
	for i in 0..64 {
		prime_idx ^= key_stream[i];
		for _ in 0..8 {
			if prime_idx & 1 != 0 {
				prime_idx = (prime_idx >> 1) ^ PRIME_IDX_XOR_MASK;
			} else {
				prime_idx >>= 1;
			}
		}
	}
	prime_idx &= 7;

	// Step 5 – decrypt using modified RC4 PRGA
	let q = PRIME_NUMBERS[prime_idx as usize];
	let mut k: u32 = 0;
	let mut n: u32 = 0;
	let mut p: u32 = 0;
	let mut xor_key: u32 = 0;
	let mut result = String::new();

	for ct in ciphertext {
		k = (k + q) % 256;
		n = (p + digest[(n as usize + digest[k as usize] as usize) % 256]) % 256;
		p = (p + k + digest[k as usize]) % 256;

		digest.swap(k as usize, n as usize);

		xor_key = digest[(n as usize
			+ digest[(k as usize
				+ digest[(xor_key as usize + p as usize) % 256] as usize)
				% 256] as usize)
			% 256];

		result.push((ct ^ xor_key) as u8 as char);
	}

	Ok(result)
}
