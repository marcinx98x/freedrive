// ========================================
// FreeDrive — E2E Key Sync (account UEK + server)
// ========================================

var CryptoSync = window.CryptoSync = (() => {
    const SYNC_CURSOR_KEY = 'fd_crypto_sync_since';
    let uekRaw = null;
    let recoveryKeyRaw = null;

    function canUse() {
        return Boolean(window.CryptoModule?.canEncrypt?.());
    }

    function isUnlocked() {
        return Boolean(uekRaw && uekRaw.length === 32);
    }

    function lock() {
        uekRaw = null;
        recoveryKeyRaw = null;
    }

    function saltFromAccount(account) {
        const salt = account?.key_salt;
        if (!salt) return null;
        if (salt instanceof Uint8Array) return salt;
        if (Array.isArray(salt)) return new Uint8Array(salt);
        if (typeof salt === 'string') {
            try {
                const bin = atob(salt);
                const out = new Uint8Array(bin.length);
                for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
                return out;
            } catch {
                return null;
            }
        }
        return null;
    }

    async function unwrapUekWithPassword(account, password) {
        const salt = saltFromAccount(account);
        if (!salt || !account?.wrapped_uek) {
            throw new Error('Encryption account is not configured on the server');
        }
        const kek = await CryptoModule.deriveKek(password, salt);
        return await CryptoModule.unwrapRawKey(account.wrapped_uek, kek);
    }

    async function unwrapUekWithRecovery(account, recoveryCode) {
        if (!account?.has_recovery || !account?.wrapped_uek_recovery) {
            throw new Error('No recovery code was set up for this account');
        }
        const recoveryKey = CryptoModule.parseRecoveryCode(recoveryCode);
        return await CryptoModule.unwrapRawKey(account.wrapped_uek_recovery, recoveryKey);
    }

    async function setupNewAccount(password) {
        const uek = CryptoModule.generateRawBytes(32);
        const recoveryKey = CryptoModule.generateRawBytes(32);
        const salt = CryptoModule.generateRawBytes(16);
        const kek = await CryptoModule.deriveKek(password, salt);
        const wrappedUek = await CryptoModule.wrapRawKey(uek, kek);
        const wrappedRecovery = await CryptoModule.wrapRawKey(uek, recoveryKey);
        await API.crypto.setupAccount({
            key_salt: Array.from(salt),
            wrapped_uek: wrappedUek,
            wrapped_uek_recovery: wrappedRecovery,
        });
        uekRaw = uek;
        recoveryKeyRaw = recoveryKey;
        return CryptoModule.formatRecoveryCode(recoveryKey);
    }

    async function unlockWithPassword(password) {
        const account = await API.crypto.getAccount();
        if (!account?.has_crypto) {
            const recoveryCode = await setupNewAccount(password);
            await syncAllKeys();
            return { setup: true, recoveryCode };
        }
        uekRaw = await unwrapUekWithPassword(account, password);
        await syncAllKeys();
        return { setup: false };
    }

    async function unlockWithRecovery(recoveryCode) {
        const account = await API.crypto.getAccount();
        if (!account?.has_crypto) {
            throw new Error('Encryption is not set up for this account');
        }
        uekRaw = await unwrapUekWithRecovery(account, recoveryCode);
        await syncAllKeys();
        return { setup: false };
    }

    async function rewrapWithNewPassword(oldPassword, newPassword) {
        const account = await API.crypto.getAccount();
        if (!account?.has_crypto) return;
        const current = await unwrapUekWithPassword(account, oldPassword);
        const salt = CryptoModule.generateRawBytes(16);
        const kek = await CryptoModule.deriveKek(newPassword, salt);
        const wrappedUek = await CryptoModule.wrapRawKey(current, kek);
        let wrappedRecovery = account.wrapped_uek_recovery || '';
        if (recoveryKeyRaw) {
            wrappedRecovery = await CryptoModule.wrapRawKey(current, recoveryKeyRaw);
        }
        await API.crypto.updateAccount({
            key_salt: Array.from(salt),
            wrapped_uek: wrappedUek,
            wrapped_uek_recovery: wrappedRecovery || undefined,
        });
        uekRaw = current;
    }

    async function rewrapAfterPasswordReset(newPassword, recoveryCode) {
        const account = await API.crypto.getAccount();
        if (!account?.has_crypto) return;
        uekRaw = await unwrapUekWithRecovery(account, recoveryCode);
        const salt = CryptoModule.generateRawBytes(16);
        const kek = await CryptoModule.deriveKek(newPassword, salt);
        const wrappedUek = await CryptoModule.wrapRawKey(uekRaw, kek);
        let wrappedRecovery = '';
        if (account.has_recovery) {
            const recoveryKey = CryptoModule.parseRecoveryCode(recoveryCode);
            recoveryKeyRaw = recoveryKey;
            wrappedRecovery = await CryptoModule.wrapRawKey(uekRaw, recoveryKey);
        }
        await API.crypto.updateAccount({
            key_salt: Array.from(salt),
            wrapped_uek: wrappedUek,
            wrapped_uek_recovery: wrappedRecovery || undefined,
        });
    }

    function getSyncCursor() {
        return localStorage.getItem(SYNC_CURSOR_KEY) || '';
    }

    function setSyncCursor(iso) {
        if (iso) localStorage.setItem(SYNC_CURSOR_KEY, iso);
    }

    async function pullKeysFromServer() {
        if (!isUnlocked()) return 0;
        let since = getSyncCursor();
        let imported = 0;
        for (;;) {
            const q = since ? `?since=${encodeURIComponent(since)}` : '';
            const data = await API.crypto.listKeys(q);
            const keys = data?.keys || [];
            if (!keys.length) break;
            for (const entry of keys) {
                if (!entry?.file_id || !entry?.wrapped_file_key) continue;
                try {
                    const raw = await CryptoModule.unwrapRawKey(entry.wrapped_file_key, uekRaw);
                    const keyB64 = CryptoModule.arrayBufferToBase64Url(raw.buffer);
                    await CryptoModule.storeKeyB64(entry.file_id, keyB64);
                    imported += 1;
                } catch {
                    /* skip invalid */
                }
                if (entry.updated_at) since = entry.updated_at;
            }
            if (keys.length < 5000) break;
        }
        if (since) setSyncCursor(since);
        return imported;
    }

    async function pushLocalKeysToServer() {
        if (!isUnlocked()) return 0;
        const exportData = await CryptoModule.exportAllKeys();
        const wrapped = {};
        for (const [fileId, keyB64] of Object.entries(exportData.keys || {})) {
            if (!keyB64) continue;
            try {
                const raw = CryptoModule.base64UrlToArrayBuffer(keyB64);
                wrapped[fileId] = await CryptoModule.wrapRawKey(new Uint8Array(raw), uekRaw);
            } catch {
                /* skip */
            }
        }
        if (!Object.keys(wrapped).length) return 0;
        const result = await API.crypto.bulkPutKeys({ keys: wrapped });
        return result?.imported || 0;
    }

    async function syncAllKeys() {
        if (!isUnlocked()) return;
        await pullKeysFromServer();
        await pushLocalKeysToServer();
        await pullKeysFromServer();
    }

    async function wrapFileKeyForUpload(key) {
        if (!isUnlocked()) return null;
        const rawB64 = await CryptoModule.exportKey(key);
        const raw = new Uint8Array(CryptoModule.base64UrlToArrayBuffer(rawB64));
        return CryptoModule.wrapRawKey(raw, uekRaw);
    }

    async function pushFileKey(fileId, key) {
        if (!isUnlocked() || !fileId || !key) return;
        const wrapped = await wrapFileKeyForUpload(key);
        if (!wrapped) return;
        await API.crypto.putFileKey(fileId, wrapped);
    }

    async function ensureFileKey(fileId) {
        let key = await CryptoModule.getKey(fileId);
        if (key) return key;
        if (!isUnlocked()) return null;
        try {
            const data = await API.crypto.getFileKey(fileId);
            if (!data?.wrapped_file_key) return null;
            const raw = await CryptoModule.unwrapRawKey(data.wrapped_file_key, uekRaw);
            key = await CryptoModule.rawKeyToCryptoKey(raw);
            await CryptoModule.storeKey(fileId, key);
            return key;
        } catch {
            return null;
        }
    }

    async function showRecoverySetupModal(recoveryCode) {
        return new Promise((resolve) => {
            Components.showModal(
                'Save your recovery code',
                `<p style="margin:0 0 12px;font-size:14px;color:#5f6368;line-height:1.45;">
                    Store this code in a safe place. You will need it to recover encrypted files
                    if you reset your password without knowing the old one.
                </p>
                <div style="font-family:monospace;font-size:15px;padding:12px;border-radius:8px;background:#f1f3f4;word-break:break-all;">${Components.escapeHtml(recoveryCode)}</div>
                <label style="display:flex;align-items:center;gap:8px;margin-top:12px;font-size:13px;color:#3c4043;">
                    <input type="checkbox" id="crypto-recovery-saved">
                    I saved this recovery code
                </label>`,
                [
                    {
                        text: 'Continue',
                        class: 'btn-primary',
                        close: false,
                        action: () => {
                            const checked = document.getElementById('crypto-recovery-saved')?.checked;
                            if (!checked) {
                                Components.toast('Confirm that you saved the recovery code', 'error');
                                return false;
                            }
                            Components.hideModal();
                            resolve(true);
                            return true;
                        },
                    },
                ],
            );
        });
    }

    async function showUnlockModal() {
        return new Promise((resolve) => {
            Components.showModal(
                'Unlock encryption',
                `<p style="margin:0 0 12px;font-size:14px;color:#5f6368;line-height:1.45;">
                    Enter your account password to access encrypted files on this device.
                </p>
                <label style="display:block;margin-bottom:12px;">
                    <span style="font-size:13px;font-weight:500;color:#5f6368;">Password</span>
                    <input id="crypto-unlock-password" type="password" autocomplete="current-password"
                        style="width:100%;height:40px;margin-top:6px;border-radius:8px;border:1px solid #dadce0;padding:0 12px;">
                </label>
                <details style="font-size:13px;color:#5f6368;">
                    <summary>Use recovery code instead</summary>
                    <input id="crypto-unlock-recovery" type="text" placeholder="xxxx-xxxx-..."
                        style="width:100%;height:40px;margin-top:8px;border-radius:8px;border:1px solid #dadce0;padding:0 12px;">
                </details>`,
                [
                    { text: 'Cancel', action: () => { resolve(false); } },
                    {
                        text: 'Unlock',
                        class: 'btn-primary',
                        close: false,
                        action: async () => {
                            const password = String(document.getElementById('crypto-unlock-password')?.value || '');
                            const recovery = String(document.getElementById('crypto-unlock-recovery')?.value || '').trim();
                            try {
                                if (recovery) {
                                    await unlockWithRecovery(recovery);
                                } else if (password) {
                                    await unlockWithPassword(password);
                                } else {
                                    Components.toast('Enter password or recovery code', 'error');
                                    return false;
                                }
                                Components.hideModal();
                                resolve(true);
                                return true;
                            } catch (err) {
                                Components.toast(err?.message || 'Unlock failed', 'error');
                                return false;
                            }
                        },
                    },
                ],
            );
        });
    }

    async function ensureUnlockedAfterLogin(password) {
        if (!canUse()) return true;
        try {
            const result = await unlockWithPassword(password);
            if (result.setup && result.recoveryCode) {
                await showRecoverySetupModal(result.recoveryCode);
            }
            return true;
        } catch (err) {
            Components.toast(err?.message || 'Encryption unlock failed', 'error');
            return false;
        }
    }

    async function ensureUnlockedOnAppLoad() {
        if (!canUse() || !API.isLoggedIn()) return true;
        if (isUnlocked()) {
            try { await syncAllKeys(); } catch { /* ignore background sync errors */ }
            return true;
        }
        return showUnlockModal();
    }

    async function buildCryptoUpdateForReset(token, email, newPassword, recoveryCode) {
        if (!recoveryCode || !canUse()) return null;
        const account = await API.auth.resetPasswordCryptoInfo(token, email);
        if (!account?.has_crypto || !account?.wrapped_uek_recovery) return null;
        const recoveryKey = CryptoModule.parseRecoveryCode(recoveryCode);
        const uek = await CryptoModule.unwrapRawKey(account.wrapped_uek_recovery, recoveryKey);
        const salt = CryptoModule.generateRawBytes(16);
        const kek = await CryptoModule.deriveKek(newPassword, salt);
        const wrappedUek = await CryptoModule.wrapRawKey(uek, kek);
        const wrappedRecovery = await CryptoModule.wrapRawKey(uek, recoveryKey);
        return {
            key_salt: Array.from(salt),
            wrapped_uek: wrappedUek,
            wrapped_uek_recovery: wrappedRecovery,
        };
    }

    return {
        canUse,
        isUnlocked,
        lock,
        unlockWithPassword,
        unlockWithRecovery,
        rewrapWithNewPassword,
        rewrapAfterPasswordReset,
        syncAllKeys,
        pushFileKey,
        ensureFileKey,
        ensureUnlockedAfterLogin,
        ensureUnlockedOnAppLoad,
        showUnlockModal,
        showRecoverySetupModal,
        buildCryptoUpdateForReset,
    };
})();
