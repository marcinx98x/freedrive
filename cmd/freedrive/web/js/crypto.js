// ========================================
// FreeDrive — WebCrypto Encryption Module
// Zero-Knowledge AES-GCM-256
// ========================================

var CryptoModule = window.CryptoModule = (() => {
    const DB_NAME = 'freedrive_keys';
    const STORE_NAME = 'encryption_keys';
    const ALGO = { name: 'AES-GCM', length: 256 };

    function getSubtleCrypto() {
        const subtle = window.crypto?.subtle;
        if (!subtle) {
            throw new Error('Browser encryption requires HTTPS or localhost. Open FreeDrive over HTTPS, or use http://localhost:8080.');
        }
        return subtle;
    }

    function getCrypto() {
        if (!window.crypto?.getRandomValues) {
            throw new Error('Browser encryption is not available in this browser.');
        }
        return window.crypto;
    }

    function canEncrypt() {
        return Boolean(window.crypto?.subtle && window.crypto?.getRandomValues);
    }

    // Open IndexedDB for key storage
    function openDB() {
        return new Promise((resolve, reject) => {
            const req = indexedDB.open(DB_NAME, 1);
            req.onupgradeneeded = () => {
                const db = req.result;
                if (!db.objectStoreNames.contains(STORE_NAME)) {
                    db.createObjectStore(STORE_NAME);
                }
            };
            req.onsuccess = () => resolve(req.result);
            req.onerror = () => reject(req.error);
        });
    }

    // Generate a new AES-GCM-256 key
    async function generateKey() {
        return await getSubtleCrypto().generateKey(
            ALGO,
            true, // extractable
            ['encrypt', 'decrypt']
        );
    }

    // Export key to base64url string
    async function exportKey(key) {
        const raw = await getSubtleCrypto().exportKey('raw', key);
        return arrayBufferToBase64Url(raw);
    }

    // Import key from base64url string
    async function importKey(base64url) {
        const raw = base64UrlToArrayBuffer(base64url);
        return await getSubtleCrypto().importKey(
            'raw',
            raw,
            ALGO,
            true,
            ['encrypt', 'decrypt']
        );
    }

    // Encrypt a file (ArrayBuffer)
    async function encryptFile(data, key) {
        const iv = getCrypto().getRandomValues(new Uint8Array(12)); // 96-bit IV
        const ciphertext = await getSubtleCrypto().encrypt(
            { name: 'AES-GCM', iv },
            key,
            data
        );
        return { ciphertext, iv };
    }

    // Decrypt a file (ArrayBuffer)
    async function decryptFile(ciphertext, key, iv) {
        return await getSubtleCrypto().decrypt(
            { name: 'AES-GCM', iv },
            key,
            ciphertext
        );
    }

    // Store key in IndexedDB
    async function storeKey(fileId, key) {
        const exported = await exportKey(key);
        const db = await openDB();
        return new Promise((resolve, reject) => {
            const tx = db.transaction(STORE_NAME, 'readwrite');
            tx.objectStore(STORE_NAME).put(exported, fileId);
            tx.oncomplete = () => resolve();
            tx.onerror = () => reject(tx.error);
        });
    }

    // Retrieve key from IndexedDB
    async function getKey(fileId) {
        const db = await openDB();
        return new Promise((resolve, reject) => {
            const tx = db.transaction(STORE_NAME, 'readonly');
            const req = tx.objectStore(STORE_NAME).get(fileId);
            req.onsuccess = async () => {
                if (req.result) {
                    const key = await importKey(req.result);
                    resolve(key);
                } else {
                    resolve(null);
                }
            };
            req.onerror = () => reject(req.error);
        });
    }

    // Delete key from IndexedDB
    async function deleteKey(fileId) {
        const db = await openDB();
        return new Promise((resolve, reject) => {
            const tx = db.transaction(STORE_NAME, 'readwrite');
            tx.objectStore(STORE_NAME).delete(fileId);
            tx.oncomplete = () => resolve();
            tx.onerror = () => reject(tx.error);
        });
    }

    // Utility: ArrayBuffer → base64url
    function arrayBufferToBase64Url(buffer) {
        const bytes = new Uint8Array(buffer);
        let binary = '';
        bytes.forEach(b => binary += String.fromCharCode(b));
        return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    }

    // Utility: base64url → ArrayBuffer
    function base64UrlToArrayBuffer(base64url) {
        const base64 = base64url.replace(/-/g, '+').replace(/_/g, '/');
        const padded = base64 + '='.repeat((4 - base64.length % 4) % 4);
        const binary = atob(padded);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
        }
        return bytes.buffer;
    }

    // Utility: Uint8Array → base64
    function uint8ToBase64(arr) {
        let binary = '';
        arr.forEach(b => binary += String.fromCharCode(b));
        return btoa(binary);
    }

    // Utility: base64 → Uint8Array
    function base64ToUint8(base64) {
        const binary = atob(base64);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
        }
        return bytes;
    }

    return {
        generateKey,
        canEncrypt,
        exportKey,
        importKey,
        encryptFile,
        decryptFile,
        storeKey,
        getKey,
        deleteKey,
        uint8ToBase64,
        base64ToUint8,
        arrayBufferToBase64Url,
        base64UrlToArrayBuffer,
    };
})();
