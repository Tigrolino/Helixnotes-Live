const SECRET_VERSION = 2;
const SECRET_CIPHER = 'AES-256-GCM';
const SECRET_KDF = 'PBKDF2-HMAC-SHA256';
const SECRET_ITERATIONS = 600_000;
const SALT_BYTES = 16;
const NONCE_BYTES = 12;

interface SecretEnvelope {
	v: number;
	title?: string;
	cipher: string;
	kdf: string;
	iterations: number;
	salt: string;
	nonce: string;
	ct: string;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function bytesToBase64(bytes: Uint8Array): string {
	let binary = '';
	for (const byte of bytes) binary += String.fromCharCode(byte);
	return btoa(binary);
}

function base64ToBytes(value: string, field: string): Uint8Array {
	const text = value.trim();
	if (!text || text.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(text)) {
		throw new Error(`Invalid secret ${field}`);
	}
	const binary = atob(text);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
	return bytes;
}

function stableBytes(bytes: Uint8Array): Uint8Array<ArrayBuffer> {
	const copy = new Uint8Array(bytes.byteLength);
	copy.set(bytes);
	return copy;
}

function authenticatedMetadata(envelope: SecretEnvelope): string {
	return JSON.stringify({
		v: envelope.v,
		title: envelope.title ?? 'Encrypted secret',
		cipher: envelope.cipher,
		kdf: envelope.kdf,
		iterations: envelope.iterations,
		salt: envelope.salt,
		nonce: envelope.nonce,
	});
}

function envelopeAad(envelope: SecretEnvelope): Uint8Array {
	return encoder.encode(authenticatedMetadata(envelope));
}

function parseEnvelope(payload: string): SecretEnvelope {
	let envelope: SecretEnvelope;
	try {
		envelope = JSON.parse(payload);
	} catch {
		throw new Error('Invalid secret payload');
	}
	if (
		envelope?.v !== SECRET_VERSION ||
		envelope.cipher !== SECRET_CIPHER ||
		envelope.kdf !== SECRET_KDF ||
		!Number.isInteger(envelope.iterations) ||
		envelope.iterations < 100_000 ||
		envelope.iterations > 5_000_000
	) {
		throw new Error('Unsupported secret payload');
	}
	return envelope;
}

export function readSecretTitle(payload: string): string {
	try {
		const parsed = JSON.parse(payload);
		return typeof parsed?.title === 'string' && parsed.title.trim()
			? parsed.title.trim()
			: 'Encrypted secret';
	} catch {
		return 'Encrypted secret';
	}
}

async function deriveKey(passphrase: string, salt: Uint8Array, iterations: number): Promise<CryptoKey> {
	if (!passphrase) throw new Error('Passphrase is required');
	const keyMaterial = await crypto.subtle.importKey(
		'raw',
		encoder.encode(passphrase),
		'PBKDF2',
		false,
		['deriveKey'],
	);
	return crypto.subtle.deriveKey(
		{ name: 'PBKDF2', hash: 'SHA-256', salt: stableBytes(salt), iterations },
		keyMaterial,
		{ name: 'AES-GCM', length: 256 },
		false,
		['encrypt', 'decrypt'],
	);
}

export async function encryptSecretText(plaintext: string, passphrase: string, title = 'Encrypted secret'): Promise<string> {
	if (!plaintext) throw new Error('Secret text is required');
	const salt = crypto.getRandomValues(new Uint8Array(SALT_BYTES));
	const nonce = crypto.getRandomValues(new Uint8Array(NONCE_BYTES));
	const key = await deriveKey(passphrase, salt, SECRET_ITERATIONS);
	const envelope: SecretEnvelope = {
		v: SECRET_VERSION,
		title: title.trim() || 'Encrypted secret',
		cipher: SECRET_CIPHER,
		kdf: SECRET_KDF,
		iterations: SECRET_ITERATIONS,
		salt: bytesToBase64(salt),
		nonce: bytesToBase64(nonce),
		ct: '',
	};
	const encrypted = await crypto.subtle.encrypt(
		{ name: 'AES-GCM', iv: stableBytes(nonce), additionalData: stableBytes(envelopeAad(envelope)) },
		key,
		encoder.encode(plaintext),
	);
	envelope.ct = bytesToBase64(new Uint8Array(encrypted));
	return JSON.stringify(envelope, null, 2);
}

export async function decryptSecretText(payload: string, passphrase: string): Promise<string> {
	try {
		const envelope = parseEnvelope(payload);
		const salt = base64ToBytes(envelope.salt, 'salt');
		const nonce = base64ToBytes(envelope.nonce, 'nonce');
		const ciphertext = base64ToBytes(envelope.ct, 'ciphertext');
		const key = await deriveKey(passphrase, salt, envelope.iterations);
		const plaintext = await crypto.subtle.decrypt(
			{ name: 'AES-GCM', iv: stableBytes(nonce), additionalData: stableBytes(envelopeAad(envelope)) },
			key,
			stableBytes(ciphertext),
		);
		return decoder.decode(plaintext);
	} catch {
		throw new Error('Unable to unlock secret. Check the passphrase or payload.');
	}
}
