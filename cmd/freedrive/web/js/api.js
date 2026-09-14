const API = (() => {
    const BASE = '/api/v1';
    const ACCOUNTS_KEY = 'fd_accounts';
    const ADDING_ACCOUNT_KEY = 'fd_adding_account';
    const RELOAD_AFTER_PASSWORD_KEY = 'fd_reload_after_password';
    const SCOPED_KEYS = [
        'fd_meta_v4',
        'fd_crypto_sync_since',
        'fd_crypto_needs_recovery',
        'fd_profile_photo',
        'fd_home_warning_dismiss_until',
    ];

    let accessToken = localStorage.getItem('fd_access_token') || '';
    let refreshToken = localStorage.getItem('fd_refresh_token') || '';
    let currentUser = JSON.parse(localStorage.getItem('fd_user') || 'null');

    function emptyVault() {
        return { activeId: '', accounts: [] };
    }

    function loadVault() {
        try {
            const raw = JSON.parse(localStorage.getItem(ACCOUNTS_KEY) || 'null');
            if (!raw || !Array.isArray(raw.accounts)) return emptyVault();
            return {
                activeId: String(raw.activeId || ''),
                accounts: raw.accounts.filter((account) => account && account.id),
            };
        } catch {
            return emptyVault();
        }
    }

    function saveVault(vault) {
        localStorage.setItem(ACCOUNTS_KEY, JSON.stringify({
            activeId: vault.activeId || '',
            accounts: vault.accounts || [],
        }));
    }

    function accountFromUser(user, tokens) {
        return {
            id: String(user.id),
            email: user.email || '',
            username: user.username || '',
            role: user.role || '',
            avatar_url: user.avatar_url || '',
            must_change_password: Boolean(user.must_change_password),
            access_token: tokens?.access_token || '',
            refresh_token: tokens?.refresh_token || '',
        };
    }

    function accountToUser(account) {
        if (!account) return null;
        return {
            id: account.id,
            email: account.email || '',
            username: account.username || '',
            role: account.role || '',
            avatar_url: account.avatar_url || '',
            must_change_password: Boolean(account.must_change_password),
        };
    }

    function writeActiveSlots(token, refresh, user) {
        accessToken = token || '';
        refreshToken = refresh || '';
        currentUser = user || null;
        if (accessToken) localStorage.setItem('fd_access_token', accessToken);
        else localStorage.removeItem('fd_access_token');
        if (refreshToken) localStorage.setItem('fd_refresh_token', refreshToken);
        else localStorage.removeItem('fd_refresh_token');
        if (currentUser) localStorage.setItem('fd_user', JSON.stringify(currentUser));
        else localStorage.removeItem('fd_user');
    }

    function applyAccount(account, user) {
        writeActiveSlots(account?.access_token, account?.refresh_token, user || accountToUser(account));
    }

    function persistActiveTokens() {
        const vault = loadVault();
        if (!vault.activeId) return;
        const idx = vault.accounts.findIndex((account) => account.id === vault.activeId);
        if (idx < 0) return;
        vault.accounts[idx].access_token = accessToken;
        vault.accounts[idx].refresh_token = refreshToken;
        if (currentUser) {
            vault.accounts[idx].email = currentUser.email || vault.accounts[idx].email;
            vault.accounts[idx].username = currentUser.username || vault.accounts[idx].username;
            vault.accounts[idx].role = currentUser.role || vault.accounts[idx].role;
            if (currentUser.avatar_url) vault.accounts[idx].avatar_url = currentUser.avatar_url;
            vault.accounts[idx].must_change_password = Boolean(currentUser.must_change_password);
        }
        saveVault(vault);
    }

    function patchVaultUser(user) {
        if (!user?.id) return;
        const vault = loadVault();
        const idx = vault.accounts.findIndex((account) => account.id === user.id);
        if (idx < 0) return;
        vault.accounts[idx] = {
            ...vault.accounts[idx],
            email: user.email || vault.accounts[idx].email,
            username: user.username || '',
            role: user.role || '',
            avatar_url: user.avatar_url || vault.accounts[idx].avatar_url || '',
            must_change_password: Boolean(user.must_change_password),
        };
        if (!vault.activeId) vault.activeId = user.id;
        saveVault(vault);
    }

    function migrateVault() {
        const vault = loadVault();
        if (vault.accounts.length) {
            if (!accessToken) {
                const next = vault.accounts.find((account) => account.id === vault.activeId) || vault.accounts[0];
                if (next) {
                    if (!vault.activeId) {
                        vault.activeId = next.id;
                        saveVault(vault);
                    }
                    applyAccount(next);
                }
            } else if (currentUser?.id && !vault.accounts.some((account) => account.id === currentUser.id)) {
                vault.accounts.push(accountFromUser(currentUser, { access_token: accessToken, refresh_token: refreshToken }));
                if (!vault.activeId) vault.activeId = currentUser.id;
                saveVault(vault);
            }
            return;
        }
        if (!accessToken || !currentUser?.id) return;
        saveVault({
            activeId: currentUser.id,
            accounts: [accountFromUser(currentUser, { access_token: accessToken, refresh_token: refreshToken })],
        });
    }

    function scopedStorageKey(base, userId) {
        const id = userId || currentUser?.id || '';
        return id ? `${base}:${id}` : base;
    }

    function getScopedItem(base, userId) {
        return localStorage.getItem(scopedStorageKey(base, userId));
    }

    function setScopedItem(base, value, userId) {
        const key = scopedStorageKey(base, userId);
        if (value == null || value === '') localStorage.removeItem(key);
        else localStorage.setItem(key, String(value));
        if (base === 'fd_profile_photo' && value && currentUser?.id && (!userId || userId === currentUser.id)) {
            currentUser.avatar_url = String(value);
            patchVaultUser(currentUser);
        }
    }

    function clearScopedStorage(userId) {
        if (!userId) return;
        SCOPED_KEYS.forEach((base) => localStorage.removeItem(`${base}:${userId}`));
    }

    function clearLegacyScopedStorage() {
        SCOPED_KEYS.forEach((base) => localStorage.removeItem(base));
    }

    function migrateScopedStorage() {
        const id = currentUser?.id;
        if (!id) return;
        SCOPED_KEYS.forEach((base) => {
            const scoped = `${base}:${id}`;
            if (localStorage.getItem(scoped) != null) return;
            const legacy = localStorage.getItem(base);
            if (legacy != null) localStorage.setItem(scoped, legacy);
        });
        try {
            const prefs = JSON.parse(localStorage.getItem('fd_user_prefs') || '{}') || {};
            if (prefs.profileAvatar && !localStorage.getItem(`fd_profile_photo:${id}`)) {
                localStorage.setItem(`fd_profile_photo:${id}`, prefs.profileAvatar);
            }
            if (prefs.profileAvatar) {
                delete prefs.profileAvatar;
                localStorage.setItem('fd_user_prefs', JSON.stringify(prefs));
            }
        } catch { /* ignore */ }
        const photo = localStorage.getItem(`fd_profile_photo:${id}`) || '';
        if (photo && currentUser && !currentUser.avatar_url) {
            currentUser.avatar_url = photo;
            localStorage.setItem('fd_user', JSON.stringify(currentUser));
            patchVaultUser(currentUser);
        }
        if (loadVault().accounts.length <= 1) clearLegacyScopedStorage();
    }

    function setTokens(tokens) {
        accessToken = tokens?.access_token || '';
        refreshToken = tokens?.refresh_token || '';
        localStorage.setItem('fd_access_token', accessToken);
        localStorage.setItem('fd_refresh_token', refreshToken);
        persistActiveTokens();
    }

    function setUser(user) {
        currentUser = user || null;
        localStorage.setItem('fd_user', JSON.stringify(currentUser));
        patchVaultUser(currentUser);
    }

    function getUser() {
        return currentUser;
    }

    function isLoggedIn() {
        return Boolean(accessToken);
    }

    function upsertAccount(user, tokens) {
        if (!user?.id || !tokens?.access_token) return;
        const vault = loadVault();
        const entry = accountFromUser(user, tokens);
        const idx = vault.accounts.findIndex((account) => account.id === user.id);
        if (idx >= 0) vault.accounts[idx] = { ...vault.accounts[idx], ...entry };
        else vault.accounts.push(entry);
        vault.activeId = user.id;
        saveVault(vault);
        applyAccount(entry, user);
    }

    function listAccounts() {
        const activeId = loadVault().activeId || currentUser?.id || '';
        return loadVault().accounts.map((account) => ({
            id: account.id,
            email: account.email,
            username: account.username,
            role: account.role,
            avatar_url: account.avatar_url,
            active: account.id === activeId,
        }));
    }

    function switchAccount(id) {
        persistActiveTokens();
        const vault = loadVault();
        const account = vault.accounts.find((entry) => entry.id === id);
        if (!account) return false;
        vault.activeId = id;
        saveVault(vault);
        applyAccount(account);
        return true;
    }

    function wipeSlots() {
        writeActiveSlots('', '', null);
    }

    function clearAuth(opts) {
        if (opts?.all) {
            const ids = loadVault().accounts.map((account) => account.id);
            ids.forEach(clearScopedStorage);
            clearLegacyScopedStorage();
            wipeSlots();
            localStorage.removeItem(ACCOUNTS_KEY);
            return { remaining: 0 };
        }
        const id = currentUser?.id || loadVault().activeId;
        const vault = loadVault();
        vault.accounts = vault.accounts.filter((account) => account.id !== id);
        if (id) clearScopedStorage(id);
        if (vault.accounts.length) {
            if (!vault.activeId || vault.activeId === id) vault.activeId = vault.accounts[0].id;
            saveVault(vault);
        } else {
            localStorage.removeItem(ACCOUNTS_KEY);
        }
        wipeSlots();
        return { remaining: vault.accounts.length };
    }

    function isAddingAccount() {
        return sessionStorage.getItem(ADDING_ACCOUNT_KEY) === '1';
    }

    function setAddingAccount(on) {
        if (on) sessionStorage.setItem(ADDING_ACCOUNT_KEY, '1');
        else sessionStorage.removeItem(ADDING_ACCOUNT_KEY);
    }

    function consumeReloadAfterPassword() {
        const pending = sessionStorage.getItem(RELOAD_AFTER_PASSWORD_KEY) === '1';
        sessionStorage.removeItem(RELOAD_AFTER_PASSWORD_KEY);
        return pending;
    }

    function markReloadAfterPassword() {
        sessionStorage.setItem(RELOAD_AFTER_PASSWORD_KEY, '1');
    }

    async function logoutRefreshToken(refresh) {
        if (!refresh) return;
        try {
            await fetch(`${BASE}/auth/logout`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ refresh_token: refresh }),
            });
        } catch { /* offline logout still drops the local session */ }
    }

    async function clearDeviceUek(userId) {
        if (!userId) return;
        try {
            if (window.CryptoSync?.lockAndClearDevice) await CryptoSync.lockAndClearDevice(userId);
            else if (window.CryptoModule?.clearDeviceUek) await CryptoModule.clearDeviceUek(userId);
        } catch { /* ignore */ }
    }

    async function signOutAccount(id) {
        const vault = loadVault();
        const account = vault.accounts.find((entry) => entry.id === id) || null;
        if (account?.refresh_token) await logoutRefreshToken(account.refresh_token);
        else if (id && id === currentUser?.id) await logoutRefreshToken(refreshToken);
        await clearDeviceUek(id);
        const wasActive = !id || id === currentUser?.id || id === vault.activeId;
        if (!wasActive && account) {
            vault.accounts = vault.accounts.filter((entry) => entry.id !== id);
            saveVault(vault);
            clearScopedStorage(id);
            return { remaining: vault.accounts.length, switched: false };
        }
        const result = clearAuth();
        return { remaining: result.remaining, switched: result.remaining > 0 };
    }

    async function signOutAll() {
        const accounts = loadVault().accounts.slice();
        for (const account of accounts) {
            await logoutRefreshToken(account.refresh_token);
            await clearDeviceUek(account.id);
        }
        clearAuth({ all: true });
        return { remaining: 0, switched: false };
    }

    function reloadActive(hash) {
        const url = new URL(window.location.href);
        url.pathname = '/';
        url.search = '';
        url.hash = hash || '#/files';
        window.location.replace(url.toString());
    }

    migrateVault();
    migrateScopedStorage();

    // Auto-refresh the access token before it expires (every 23 hours)
    let _refreshTimer = null;
    function startAutoRefresh() {
        if (_refreshTimer) clearInterval(_refreshTimer);
        if (!refreshToken) return;
        // Refresh every 23 hours (token lasts 24h)
        _refreshTimer = setInterval(async () => {
            if (refreshToken) await tryRefresh();
        }, 23 * 60 * 60 * 1000);
    }
    // Also try to refresh immediately on page load if we have a refresh token
    if (refreshToken) {
        setTimeout(() => tryRefresh().then(ok => { if (ok) startAutoRefresh(); }), 2000);
    }

    function getDeviceID() {
        let id = localStorage.getItem('fd_device_id') || '';
        if (!id) {
            id = (typeof crypto !== 'undefined' && crypto.randomUUID)
                ? crypto.randomUUID()
                : ('web-' + Date.now().toString(36) + '-' + Math.random().toString(36).slice(2, 10));
            localStorage.setItem('fd_device_id', id);
        }
        return id;
    }

    async function request(method, path, body = null, isRetry = false, rlRetries = 2, extraHeaders = null) {
        const headers = {};
        if (!(body instanceof FormData)) {
            headers['Content-Type'] = 'application/json';
        }
        headers['X-Device-ID'] = getDeviceID();
        if (accessToken) {
            headers.Authorization = `Bearer ${accessToken}`;
        }
        if (extraHeaders && typeof extraHeaders === 'object') {
            Object.assign(headers, extraHeaders);
        }

        const opts = { method, headers };
        if (body && method !== 'GET') {
            opts.body = body instanceof FormData ? body : JSON.stringify(body);
        }

        let res;
        try {
            res = await fetch(BASE + path, opts);
        } catch (err) {
            const raw = String(err?.message || '').toLowerCase();
            if (raw.includes('failed to fetch') || raw.includes('networkerror') || raw.includes('load failed') || !raw) {
                throw new Error('Cannot reach the server. Check the FreeDrive URL (HTTPS / reverse proxy) and try again.');
            }
            throw new Error(err.message || 'Network error');
        }

        // Public auth endpoints return 401 for bad credentials — never treat as session refresh.
        const isPublicAuth = path === '/auth/login'
            || path === '/auth/register'
            || path === '/auth/refresh'
            || path === '/auth/reset-password'
            || path === '/auth/confirm-email'
            || path === '/auth/verify-2fa'
            || path === '/auth/2fa/send-email'
            || path === '/auth/forgot-password'
            || path.startsWith('/auth/login-approval/');
        if (res.status === 401 && !isRetry && refreshToken && !isPublicAuth) {
            const refreshed = await tryRefresh();
            if (refreshed) return request(method, path, body, true, rlRetries, extraHeaders);
            const { remaining } = clearAuth();
            if (remaining) reloadActive('#/files');
            else window.location.hash = '#/login';
            throw new Error('Session expired');
        }

        if (res.status === 429 && rlRetries > 0) {
            await new Promise((r) => setTimeout(r, 400));
            return request(method, path, body, isRetry, rlRetries - 1, extraHeaders);
        }

        const data = await res.json().catch(() => ({}));
        if (res.status === 403 && data.must_change_password) {
            const user = getUser() || {};
            user.must_change_password = true;
            setUser(user);
            if (typeof Auth !== 'undefined' && Auth.showForcePasswordForm) {
                Auth.showForcePasswordForm();
            }
            throw new Error(data.error || 'password change required');
        }
        if (!res.ok) throw new Error(data.error || `Request failed (${res.status})`);
        return data;
    }

    async function tryRefresh() {
        try {
            const res = await fetch(`${BASE}/auth/refresh`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ refresh_token: refreshToken }),
            });
            if (!res.ok) return false;
            const data = await res.json();
            setTokens(data.tokens);
            return true;
        } catch {
            return false;
        }
    }

    function uploadXHR(path, formData, onProgress) {
        return new Promise((resolve, reject) => {
            const xhr = new XMLHttpRequest();
            xhr.open('POST', BASE + path);
            if (accessToken) xhr.setRequestHeader('Authorization', `Bearer ${accessToken}`);
            xhr.setRequestHeader('X-Device-ID', getDeviceID());

            xhr.upload.onprogress = (e) => {
                if (!e.lengthComputable || !onProgress) return;
                onProgress(Math.round((e.loaded / e.total) * 100));
            };

            xhr.onload = () => {
                let payload = {};
                try { payload = JSON.parse(xhr.responseText || '{}'); } catch {}
                if (xhr.status >= 200 && xhr.status < 300) {
                    resolve(payload);
                } else {
                    reject(new Error(payload.error || `Upload failed (${xhr.status})`));
                }
            };
            xhr.onerror = () => reject(new Error('Network error'));
            xhr.send(formData);
        });
    }

    /** Ciphertext larger than this uses resumable chunked upload (Cloudflare-safe). */
    const RESUMABLE_THRESHOLD = 32 * 1024 * 1024;
    const RESUMABLE_CHUNK = 8 * 1024 * 1024;

    function putChunkXHR(sessionId, body, contentRange, onChunkProgress) {
        return new Promise((resolve, reject) => {
            const xhr = new XMLHttpRequest();
            xhr.open('PUT', `${BASE}/uploads/sessions/${sessionId}`);
            if (accessToken) xhr.setRequestHeader('Authorization', `Bearer ${accessToken}`);
            xhr.setRequestHeader('X-Device-ID', getDeviceID());
            xhr.setRequestHeader('Content-Range', contentRange);
            xhr.setRequestHeader('Content-Type', 'application/octet-stream');
            if (onChunkProgress) {
                xhr.upload.onprogress = (e) => {
                    if (e.lengthComputable) onChunkProgress(e.loaded, e.total);
                };
            }
            xhr.onload = () => {
                let payload = {};
                try { payload = JSON.parse(xhr.responseText || '{}'); } catch {}
                if (xhr.status >= 200 && xhr.status < 300) resolve(payload);
                else reject(new Error(payload.error || `Chunk upload failed (${xhr.status})`));
            };
            xhr.onerror = () => reject(new Error('Network error'));
            xhr.send(body);
        });
    }

    /**
     * Upload encrypted (or plain) bytes via resumable session when large enough.
     * @param {object} opts
     * @param {ArrayBuffer|Uint8Array|Blob} opts.data
     * @param {string} opts.name
     * @param {string} opts.mimeType
     * @param {number} opts.originalSize
     * @param {string} [opts.iv]
     * @param {string} [opts.folderId]
     * @param {string} [opts.fileId] replace existing file content
     * @param {string} [opts.contentHash] SHA-256 hex of plaintext (skips version when unchanged)
     * @param {boolean} [opts.forceVersion] always keep a historical version snapshot
     * @param {(pct:number)=>void} [opts.onProgress]
     */
    async function uploadBytes(opts) {
        const {
            data, name, mimeType, originalSize, iv = '', folderId, fileId,
            contentHash = '', forceVersion = false, onProgress,
        } = opts;
        let bytes;
        if (data instanceof Blob) {
            bytes = new Uint8Array(await data.arrayBuffer());
        } else if (data instanceof ArrayBuffer) {
            bytes = new Uint8Array(data);
        } else {
            bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
        }
        const encryptedSize = bytes.byteLength;

        if (encryptedSize <= RESUMABLE_THRESHOLD) {
            const form = new FormData();
            form.append('name', name);
            form.append('mime_type', mimeType || 'application/octet-stream');
            form.append('original_size', String(originalSize));
            if (iv) form.append('iv', iv);
            if (folderId) form.append('folder_id', folderId);
            if (contentHash) form.append('content_hash', contentHash);
            if (forceVersion) form.append('force_version', '1');
            form.append('file', new Blob([bytes], { type: 'application/octet-stream' }), name);
            if (fileId) return files.updateContent(fileId, form, onProgress);
            return uploadXHR('/files/upload', form, onProgress);
        }

        const sessionBody = {
            name,
            mime_type: mimeType || 'application/octet-stream',
            iv: iv || '',
            original_size: originalSize,
            encrypted_size: encryptedSize,
        };
        if (folderId) sessionBody.folder_id = folderId;
        if (fileId) sessionBody.file_id = fileId;
        if (contentHash) sessionBody.content_hash = contentHash;
        if (forceVersion) sessionBody.force_version = true;

        const session = await request('POST', '/uploads/sessions', sessionBody);
        if (session?.unchanged && session.file) {
            return session.file;
        }
        let offset = 0;
        let last = null;
        while (offset < encryptedSize) {
            const end = Math.min(offset + RESUMABLE_CHUNK, encryptedSize) - 1;
            const chunk = bytes.subarray(offset, end + 1);
            const range = `bytes ${offset}-${end}/${encryptedSize}`;
            last = await putChunkXHR(session.id, chunk, range, (loaded, total) => {
                if (!onProgress) return;
                const overall = offset + (total ? (loaded / total) * (end - offset + 1) : 0);
                onProgress(Math.min(99, Math.round((overall / encryptedSize) * 100)));
            });
            offset = end + 1;
            if (onProgress) onProgress(Math.min(100, Math.round((offset / encryptedSize) * 100)));
            if (last && last.id && last.name && last.complete !== false && offset >= encryptedSize) {
                return last;
            }
            if (last && last.complete === false) continue;
            if (last && last.id && last.owner_id !== undefined) return last;
            if (last && last.id && last.mime_type !== undefined && offset >= encryptedSize) return last;
        }
        return last;
    }

    async function downloadBlob(fileId) {
        const res = await fetch(`${BASE}/files/${fileId}/download`, {
            headers: { Authorization: `Bearer ${accessToken}` },
        });
        if (!res.ok) throw new Error('Download failed');
        const blob = await res.blob();
        return {
            blob,
            iv: res.headers.get('X-File-IV') || '',
            mime: res.headers.get('X-File-Mime') || blob.type,
            originalSize: Number(res.headers.get('X-Original-Size') || 0),
        };
    }

    async function downloadBlobVersion(fileId, version) {
        const res = await fetch(`${BASE}/files/${fileId}/versions/${encodeURIComponent(version)}/download`, {
            headers: { Authorization: `Bearer ${accessToken}` },
        });
        if (!res.ok) throw new Error('Download failed');
        const blob = await res.blob();
        return {
            blob,
            iv: res.headers.get('X-File-IV') || '',
            mime: res.headers.get('X-File-Mime') || blob.type,
            originalSize: Number(res.headers.get('X-Original-Size') || 0),
        };
    }

    const auth = {
        login: (email, password) => request('POST', '/auth/login', { email, password }),
        pollLoginApproval: (id, token) => request(
            'GET',
            `/auth/login-approval/${encodeURIComponent(id)}`,
            null,
            false,
            2,
            { 'X-Login-Approval-Token': token },
        ),
        verify2FA: (challenge_id, code) => request('POST', '/auth/verify-2fa', { challenge_id, code }),
        send2FAEmail: (challenge_id) => request('POST', '/auth/2fa/send-email', { challenge_id }),
        register: (email, username, password, invite_code) => request('POST', '/auth/register', { email, username, password, invite_code }),
        logout: () => request('POST', '/auth/logout', { refresh_token: refreshToken }),
        resetPassword: (token, email, new_password, crypto_update) => request('POST', '/auth/reset-password', {
            token, email, new_password, crypto_update: crypto_update || undefined,
        }),
        resetPasswordCryptoInfo: (token, email) => request('POST', '/auth/reset-password/crypto-info', { token, email }),
        forgotPassword: (email) => request('POST', '/auth/forgot-password', { email }),
        confirmEmail: (token) => request('POST', '/auth/confirm-email', { token }),
        getSessions: () => request('GET', '/auth/sessions'),
        revokeSession: (id) => request('DELETE', `/auth/sessions/${id}`),
        revokeOtherSessions: () => request('POST', '/auth/sessions/revoke-others'),
    };

    const files = {
        list: (params = {}) => {
            const q = new URLSearchParams(params).toString();
            return request('GET', `/files${q ? `?${q}` : ''}`);
        },
        get: (id) => request('GET', `/files/${id}`),
        update: (id, data) => request('PATCH', `/files/${id}`, data),
        delete: (id) => request('DELETE', `/files/${id}`),
        restore: (id) => request('POST', `/files/${id}/restore`),
        permanentDelete: (id) => request('DELETE', `/files/${id}/permanent`),
        versions: (id) => request('GET', `/files/${id}/versions`),
        trash: () => request('GET', '/files/trash'),
        upload: (formData, onProgress) => uploadXHR('/files/upload', formData, onProgress),
        updateContent: (id, formData, onProgress) => uploadXHR(`/files/${id}/content`, formData, onProgress),
        restoreVersion: (id, version) => request('POST', `/files/${id}/versions/${version}/restore`),
    };

    const folders = {
        create: (name, parentId, color) => request('POST', '/folders', { name, parent_id: parentId || null, color }),
        get: (id, opts = {}) => {
            const q = new URLSearchParams();
            if (opts.page_size) q.set('page_size', String(opts.page_size));
            if (opts.page_token) q.set('page_token', opts.page_token);
            const qs = q.toString();
            return request('GET', `/folders/${id}${qs ? `?${qs}` : ''}`);
        },
        root: (opts = {}) => {
            const q = new URLSearchParams();
            if (opts.page_size) q.set('page_size', String(opts.page_size));
            if (opts.page_token) q.set('page_token', opts.page_token);
            const qs = q.toString();
            return request('GET', `/folders/root${qs ? `?${qs}` : ''}`);
        },
        /** Load every page of files (folders come on the first page). */
        getAll: async (id) => {
            const pageSize = 500;
            let page_token = '';
            let folder = null;
            let childFolders = [];
            const files = [];
            let total_files = 0;
            let guard = 0;
            while (guard < 10000) {
                guard += 1;
                const opts = { page_size: pageSize };
                if (page_token) opts.page_token = page_token;
                const data = id ? await API.folders.get(id, opts) : await API.folders.root(opts);
                if (!folder && data.folder) folder = data.folder;
                if (Array.isArray(data.folders) && data.folders.length) childFolders = data.folders;
                if (typeof data.total_files === 'number') total_files = data.total_files;
                if (Array.isArray(data.files)) files.push(...data.files);
                page_token = data.next_page_token || '';
                if (!page_token) break;
            }
            return { folder, folders: childFolders, files, total_files, next_page_token: '' };
        },
        all: (search) => request('GET', `/folders/all${search ? `?search=${encodeURIComponent(search)}` : ''}`),
        update: (id, data) => request('PATCH', `/folders/${id}`, data),
        delete: (id) => request('DELETE', `/folders/${id}`),
        restore: (id) => request('POST', `/folders/${id}/restore`),
        permanentDelete: (id) => request('DELETE', `/folders/${id}/permanent`),
        trash: () => request('GET', '/folders/trash'),
        breadcrumb: (id) => request('GET', `/folders/${id}/breadcrumb`),
    };

    const computers = {
        list: () => request('GET', '/computers'),
        get: (id) => request('GET', `/computers/${id}`),
        register: (name, hostname) => request('POST', '/computers/register', { name, hostname }),
        heartbeat: (id) => request('POST', `/computers/${id}/heartbeat`),
        delete: (id) => request('DELETE', `/computers/${id}`),
    };

    const trash = {
        empty: () => request('POST', '/trash/empty'),
    };

    const shares = {
        sharedWithMe: () => request('GET', '/shares/with-me'),
        sharedByMe: () => request('GET', '/shares/by-me'),
        createUserShare: (data) => request('POST', '/shares/users', data),
        send: (data) => request('POST', '/shares/send', data),
        getSettings: (query) => {
            const params = new URLSearchParams();
            if (query?.file_id) params.set('file_id', query.file_id);
            if (query?.folder_id) params.set('folder_id', query.folder_id);
            return request('GET', `/shares/settings?${params.toString()}`);
        },
        saveSettings: (data) => request('PUT', '/shares/settings', data),
        access: (query) => {
            const params = new URLSearchParams();
            if (query?.file_id) params.set('file_id', query.file_id);
            if (query?.folder_id) params.set('folder_id', query.folder_id);
            return request('GET', `/shares/access?${params.toString()}`);
        },
        updateUserShare: (id, data) => request('PATCH', `/shares/users/${id}`, data),
        deleteUserShare: (id) => request('DELETE', `/shares/users/${id}`),
        listLinks: () => request('GET', '/shares/links'),
        createLink: (data) => request('POST', '/shares/links', data),
        deleteLink: (id) => request('DELETE', `/shares/links/${id}`),
        /** Password-protected public links must send X-Share-Password (never ?password=). */
        publicInfo: async (token, password = '') => {
            const headers = {};
            if (password) headers['X-Share-Password'] = password;
            const res = await fetch(`${BASE}/public/share/${encodeURIComponent(token)}`, { headers });
            if (!res.ok) {
                const err = await res.json().catch(() => ({}));
                throw new Error(err.error || 'invalid or expired share link');
            }
            return res.json();
        },
    };

    const comments = {
        list: (fileId) => request('GET', `/files/${fileId}/comments`),
        create: (fileId, data) => request('POST', `/files/${fileId}/comments`, data),
        delete: (fileId, commentId) => request('DELETE', `/files/${fileId}/comments/${commentId}`),
    };

    const diskStats = () => request('GET', '/disk-stats');
    const myStorage = () => request('GET', '/me/storage');
    const me = () => request('GET', '/me');
    const updateMe = (payload) => request('PATCH', '/me', payload);
    const loginApprovalStatus = () => request('GET', '/me/login-approval/status');
    const totpSetup = () => request('POST', '/me/totp/setup');
    const totpConfirm = (code) => request('POST', '/me/totp/confirm', { code });
    const totpDisable = (payload) => request('POST', '/me/totp/disable', payload || {});
    const requestEmailChange = (new_email, password) => request('POST', '/me/email-change/request', { new_email, password });
    const emailChangeStatus = () => request('GET', '/me/email-change/status');

    const admin = {
        users: () => request('GET', '/admin/users'),
        userAvatars: () => request('GET', '/admin/users/avatars'),
        createUser: (data) => request('POST', '/admin/users', data),
        updateUser: (id, data) => request('PATCH', `/admin/users/${id}`, data),
        deleteUser: (id) => request('DELETE', `/admin/users/${id}`),
        sendPasswordReset: (id) => request('POST', `/admin/users/${id}/reset-password`),
        send2FAReminder: () => request('POST', '/admin/users/send-2fa-reminder'),
        revokeUserSessions: (id) => request('POST', `/admin/users/${id}/revoke-sessions`),
        sessions: () => request('GET', '/admin/sessions'),
        revokeAllSessions: () => request('POST', '/admin/sessions/revoke-all'),
        forcePasswordResetAll: () => request('POST', '/admin/users/force-password-reset'),
        stats: () => request('GET', '/admin/stats'),
        createInvite: (data) => request('POST', '/admin/invites', data),
        resendInvite: (data) => request('POST', '/admin/invites/resend', data),
        invites: () => request('GET', '/admin/invites'),
        deleteInvite: (id) => request('DELETE', `/admin/invites/${id}`),
        activity: (page = 1, pageSize = 50) => request('GET', `/admin/activity?page=${page}&page_size=${pageSize}`),
        purgeTrash: (days = 30) => request('POST', `/admin/storage/purge-trash?days=${encodeURIComponent(days)}`),
        storageBreakdown: (userId) => {
            const q = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
            return request('GET', `/admin/storage/breakdown${q}`);
        },
        listDuplicates: () => request('GET', '/admin/storage/duplicates'),
        purgeDuplicates: () => request('POST', '/admin/storage/duplicates/purge'),
        listBackups: () => request('GET', '/admin/backup/list'),
        downloadBackup: async (filename) => {
            const res = await fetch(`${BASE}/admin/backup/download/${encodeURIComponent(filename)}`, {
                headers: { Authorization: `Bearer ${accessToken}` },
            });
            if (!res.ok) {
                const data = await res.json().catch(() => ({}));
                throw new Error(data.error || `Download failed (${res.status})`);
            }
            const blob = await res.blob();
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = filename;
            document.body.appendChild(a);
            a.click();
            a.remove();
            URL.revokeObjectURL(url);
        },
        restoreBackup: (filename) => request('POST', '/admin/backup/restore', { filename }),
        deleteBackup: (filename) => request('DELETE', `/admin/backup/${encodeURIComponent(filename)}`),
    };

    const activity = {
        list: (page = 1, pageSize = 50) => request('GET', `/activity?page=${page}&page_size=${pageSize}`),
    };

    const search = {
        advanced: (params = {}) => {
            const q = new URLSearchParams();
            Object.entries(params).forEach(([key, value]) => {
                if (value !== undefined && value !== null && value !== '') {
                    q.set(key, String(value));
                }
            });
            const qs = q.toString();
            return request('GET', `/search${qs ? `?${qs}` : ''}`);
        },
    };

    const approvals = {
        list: (status = '') => request('GET', `/approvals${status ? `?status=${encodeURIComponent(status)}` : ''}`),
        create: (fileId, data) => request('POST', `/files/${fileId}/approvals`, data),
        update: (id, data) => request('PATCH', `/approvals/${id}`, data),
    };

    const crypto = {
        getAccount: () => request('GET', '/crypto/account'),
        setupAccount: (data) => request('POST', '/crypto/account', data),
        updateAccount: (data) => request('PUT', '/crypto/account', data),
        listKeys: (query = '') => request('GET', `/encryption-keys${query}`),
        getFileKey: (fileId) => request('GET', `/files/${fileId}/encryption-key`),
        putFileKey: (fileId, wrappedFileKey) => request('PUT', `/files/${fileId}/encryption-key`, { wrapped_file_key: wrappedFileKey }),
        bulkPutKeys: (data) => request('POST', '/encryption-keys/bulk', data),
    };

    return {
        setTokens,
        setUser,
        getUser,
        isLoggedIn,
        clearAuth,
        upsertAccount,
        listAccounts,
        switchAccount,
        signOutAccount,
        signOutAll,
        isAddingAccount,
        setAddingAccount,
        consumeReloadAfterPassword,
        markReloadAfterPassword,
        scopedStorageKey,
        getScopedItem,
        setScopedItem,
        reloadActive,
        auth,
        files,
        folders,
        computers,
        trash,
        shares,
        comments,
        admin,
        activity,
        search,
        approvals,
        crypto,
        diskStats,
        myStorage,
        me,
        updateMe,
        changePassword: (current_password, new_password, crypto_update) => request('POST', '/me/password', {
            current_password,
            new_password,
            crypto_update: crypto_update || undefined,
        }),
        loginApprovalStatus,
        totpSetup,
        totpConfirm,
        totpDisable,
        requestEmailChange,
        emailChangeStatus,
        request,
        uploadFile: uploadXHR.bind(null, '/files/upload'),
        uploadBytes,
        downloadBlob,
        downloadBlobVersion,
    };
})();
