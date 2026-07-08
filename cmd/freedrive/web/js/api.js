const API = (() => {
    const BASE = '/api/v1';
    let accessToken = localStorage.getItem('fd_access_token') || '';
    let refreshToken = localStorage.getItem('fd_refresh_token') || '';
    let currentUser = JSON.parse(localStorage.getItem('fd_user') || 'null');

    function setTokens(tokens) {
        accessToken = tokens?.access_token || '';
        refreshToken = tokens?.refresh_token || '';
        localStorage.setItem('fd_access_token', accessToken);
        localStorage.setItem('fd_refresh_token', refreshToken);
    }

    function setUser(user) {
        currentUser = user || null;
        localStorage.setItem('fd_user', JSON.stringify(currentUser));
    }

    function getUser() {
        return currentUser;
    }

    function isLoggedIn() {
        return Boolean(accessToken);
    }

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

    function clearAuth() {
        accessToken = '';
        refreshToken = '';
        currentUser = null;
        localStorage.removeItem('fd_access_token');
        localStorage.removeItem('fd_refresh_token');
        localStorage.removeItem('fd_user');
    }

    async function request(method, path, body = null, isRetry = false, rlRetries = 2) {
        const headers = {};
        if (!(body instanceof FormData)) {
            headers['Content-Type'] = 'application/json';
        }
        if (accessToken) {
            headers.Authorization = `Bearer ${accessToken}`;
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

        if (res.status === 401 && !isRetry && refreshToken) {
            const refreshed = await tryRefresh();
            if (refreshed) return request(method, path, body, true, rlRetries);
            clearAuth();
            window.location.hash = '#/login';
            throw new Error('Session expired');
        }

        if (res.status === 429 && rlRetries > 0) {
            await new Promise((r) => setTimeout(r, 400));
            return request(method, path, body, isRetry, rlRetries - 1);
        }

        const data = await res.json().catch(() => ({}));
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

    const auth = {
        login: (email, password) => request('POST', '/auth/login', { email, password }),
        register: (email, username, password, invite_code) => request('POST', '/auth/register', { email, username, password, invite_code }),
        logout: () => request('POST', '/auth/logout', { refresh_token: refreshToken }),
        resetPassword: (token, email, new_password) => request('POST', '/auth/reset-password', { token, email, new_password }),
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
        get: (id) => request('GET', `/folders/${id}`),
        root: () => request('GET', '/folders/root'),
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
    };

    const diskStats = () => request('GET', '/disk-stats');
    const myStorage = () => request('GET', '/me/storage');

    const admin = {
        users: () => request('GET', '/admin/users'),
        createUser: (data) => request('POST', '/admin/users', data),
        updateUser: (id, data) => request('PATCH', `/admin/users/${id}`, data),
        deleteUser: (id) => request('DELETE', `/admin/users/${id}`),
        sendPasswordReset: (id) => request('POST', `/admin/users/${id}/reset-password`),
        stats: () => request('GET', '/admin/stats'),
        createInvite: (data) => request('POST', '/admin/invites', data),
        resendInvite: (data) => request('POST', '/admin/invites/resend', data),
        invites: () => request('GET', '/admin/invites'),
        activity: (page = 1, pageSize = 50) => request('GET', `/admin/activity?page=${page}&page_size=${pageSize}`),
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
    };

    return {
        setTokens,
        setUser,
        getUser,
        isLoggedIn,
        clearAuth,
        auth,
        files,
        folders,
        computers,
        admin,
        activity,
        search,
        approvals,
        diskStats,
        myStorage,
        request,
        uploadFile: uploadXHR.bind(null, '/files/upload'),
        downloadBlob,
    };
})();
