const App = (() => {
    const USER_PREFS_KEY = 'fd_user_prefs';

    function getUserPrefs() {
        try {
            return JSON.parse(localStorage.getItem(USER_PREFS_KEY) || '{}') || {};
        } catch {
            return {};
        }
    }

    function setUserPrefs(next) {
        localStorage.setItem(USER_PREFS_KEY, JSON.stringify(next || {}));
    }

    function resolveAvatar(user, prefs) {
        const fromUser = String(user?.avatar_url || '').trim();
        if (fromUser) return fromUser;
        const scoped = API.getScopedItem ? String(API.getScopedItem('fd_profile_photo') || '').trim() : '';
        if (scoped) return scoped;
        const fromPrefs = String(prefs?.profileAvatar || '').trim();
        if (fromPrefs) return fromPrefs;
        return '';
    }

    function syncAvatarCache(avatarUrl) {
        const prefs = getUserPrefs();
        if (prefs.profileAvatar) {
            const next = { ...prefs };
            delete next.profileAvatar;
            setUserPrefs(next);
        }
        if (API.setScopedItem) API.setScopedItem('fd_profile_photo', avatarUrl || '');
        else if (avatarUrl) localStorage.setItem('fd_profile_photo', avatarUrl);
        else localStorage.removeItem('fd_profile_photo');
    }

    function resizeAvatarDataURL(dataUrl, maxSize = 256, quality = 0.85) {
        return new Promise((resolve) => {
            if (!dataUrl || !String(dataUrl).startsWith('data:image/')) {
                resolve(dataUrl || '');
                return;
            }
            const img = new Image();
            img.onload = () => {
                const scale = Math.min(1, maxSize / Math.max(img.width, img.height));
                const w = Math.max(1, Math.round(img.width * scale));
                const h = Math.max(1, Math.round(img.height * scale));
                const canvas = document.createElement('canvas');
                canvas.width = w;
                canvas.height = h;
                const ctx = canvas.getContext('2d');
                if (!ctx) {
                    resolve(dataUrl);
                    return;
                }
                ctx.drawImage(img, 0, 0, w, h);
                try {
                    resolve(canvas.toDataURL('image/jpeg', quality));
                } catch {
                    resolve(dataUrl);
                }
            };
            img.onerror = () => resolve(dataUrl);
            img.src = dataUrl;
        });
    }

    async function refreshProfileFromServer() {
        try {
            const user = await API.me();
            if (user?.id) {
                API.setUser(user);
                syncAvatarCache(user.avatar_url || '');
                return user;
            }
        } catch {
            /* ignore */
        } finally {
            refreshUserUI();
        }
        return null;
    }

    function isAdminUser(user) {
        return String(user?.role || '').toLowerCase() === 'admin';
    }

    function syncAdminBtnVisibility() {
        const app = document.getElementById('app');
        const item = document.getElementById('settings-menu-admin');
        if (!app || !item) return;

        const user = API.getUser();
        const inDriveMode = !app.classList.contains('admin-mode');
        const show = inDriveMode && isAdminUser(user);

        item.classList.toggle('hidden', !show);
        item.setAttribute('aria-hidden', show ? 'false' : 'true');
    }

    function profileDisplayName(user) {
        if (!user) return 'User';
        const username = String(user.username || '').trim();
        if (username) return username;
        const email = String(user.email || '');
        const at = email.indexOf('@');
        return at > 0 ? email.slice(0, at) : (email || 'User');
    }

    async function refreshProfileStorage() {
        const storageEl = document.getElementById('profile-storage');
        const textEl = document.getElementById('profile-storage-text');
        const warnEl = document.getElementById('profile-storage-warn');
        const fillEl = document.getElementById('profile-storage-bar-fill');
        if (!storageEl || !textEl || !fillEl) return;

        try {
            const stats = await API.myStorage();
            const used = Number(stats?.used_bytes || 0);
            const total = Number(stats?.total_bytes || 0);
            if (!(total > 0)) {
                storageEl.classList.add('hidden');
                return;
            }
            const pct = Math.min(100, Math.round((used / total) * 100));
            textEl.textContent = `${pct}% of ${Components.formatSize(total)} used`;
            fillEl.style.width = `${pct}%`;
            warnEl?.classList.toggle('hidden', pct < 80);
            storageEl.classList.remove('hidden');
        } catch {
            storageEl.classList.add('hidden');
        }
    }

    function populateProfileDropdown() {
        const user = API.getUser?.() || {};
        const displayName = profileDisplayName(user);
        const greeting = document.getElementById('profile-greeting');
        const emailEl = document.getElementById('profile-email');
        if (greeting) greeting.textContent = displayName;
        if (emailEl) emailEl.textContent = user.email || '';

        const initial = Components.initials(displayName || user.email || 'U');
        const lgAvatar = document.getElementById('profile-avatar-lg');
        const prefs = getUserPrefs();
        const savedPhoto = resolveAvatar(user, prefs);

        if (lgAvatar) {
            if (savedPhoto) {
                lgAvatar.textContent = '';
                lgAvatar.style.backgroundImage = `url(${savedPhoto})`;
                lgAvatar.style.backgroundSize = 'cover';
                lgAvatar.style.backgroundPosition = 'center';
            } else {
                lgAvatar.style.backgroundImage = '';
                lgAvatar.textContent = initial;
            }
        }

        refreshProfileStorage();
        renderAccountSwitcher();
    }

    function setAccountsExpanded(expanded) {
        const card = document.getElementById('profile-account-card');
        const panel = document.getElementById('profile-accounts-panel');
        const toggle = document.getElementById('profile-accounts-toggle');
        card?.classList.toggle('is-expanded', expanded);
        if (panel) panel.hidden = !expanded;
        if (toggle) {
            toggle.setAttribute('aria-expanded', expanded ? 'true' : 'false');
            toggle.setAttribute('aria-label', expanded ? 'Hide other accounts' : 'Show other accounts');
        }
    }

    function renderAccountSwitcher() {
        const list = document.getElementById('profile-accounts');
        const signOutAll = document.getElementById('signout-all-btn');
        const accounts = API.listAccounts?.() || [];
        const others = accounts.filter((account) => !account.active);
        if (signOutAll) signOutAll.classList.toggle('hidden', accounts.length < 2);
        if (!list) return;
        if (!others.length) {
            list.innerHTML = '';
            return;
        }
        list.innerHTML = others.map((account) => {
            const name = Components.escapeHtml(account.username || account.email || 'Account');
            const email = Components.escapeHtml(account.email || '');
            const initial = Components.escapeHtml(Components.initials(account.username || account.email || 'U'));
            const photo = account.avatar_url
                ? `<img alt="" src="${Components.escapeHtml(account.avatar_url)}">`
                : initial;
            return `<button type="button" class="profile-account-row" data-account-id="${Components.escapeHtml(account.id)}">
                <span class="profile-account-avatar">${photo}</span>
                <span class="profile-account-text">
                    <span class="profile-account-name">${name}</span>
                    <span class="profile-account-email">${email}</span>
                </span>
            </button>`;
        }).join('');
        const expanded = document.getElementById('profile-account-card')?.classList.contains('is-expanded');
        setAccountsExpanded(Boolean(expanded));
    }

    async function beginAddAccount() {
        document.getElementById('profile-dropdown')?.classList.add('hidden');
        if (window.CryptoModule?.migrateUnscopedFileKeys) {
            try { await CryptoModule.migrateUnscopedFileKeys(); } catch { /* cache miss is recoverable */ }
        }
        API.setAddingAccount?.(true);
        showAuth();
        Auth.showLoginForm?.();
    }

    async function switchToAccount(id) {
        if (!id || !API.switchAccount?.(id)) return;
        API.reloadActive?.('#/files');
    }

    function refreshUserUI() {
        const user = API.getUser();
        if (user) {
            const prefs = getUserPrefs();
            const initial = Components.initials(user.username || user.email || 'U');
            const ua = document.getElementById('user-avatar');
            const ta = document.getElementById('topbar-avatar');
            const savedPhoto = resolveAvatar(user, prefs);
            
            if (ua) {
                ua.textContent = savedPhoto ? '' : initial;
                ua.style.backgroundImage = savedPhoto ? `url(${savedPhoto})` : '';
                ua.style.backgroundSize = savedPhoto ? 'cover' : '';
                ua.style.backgroundPosition = savedPhoto ? 'center' : '';
                ua.style.color = savedPhoto ? 'transparent' : '';
            }
            if (ta) {
                ta.innerHTML = '';
                ta.textContent = savedPhoto ? '' : initial;
                ta.style.backgroundImage = savedPhoto ? `url(${savedPhoto})` : '';
                ta.style.backgroundSize = savedPhoto ? 'cover' : '';
                ta.style.backgroundPosition = savedPhoto ? 'center' : '';
                ta.style.color = savedPhoto ? 'transparent' : '';
            }
            const un = document.getElementById('user-name');
            if (un) un.textContent = user.username || user.email;
            const ur = document.getElementById('user-role');
            if (ur) ur.textContent = user.role;

            const greeting = document.getElementById('profile-greeting');
            if (greeting) greeting.textContent = profileDisplayName(user);
            const emailEl = document.getElementById('profile-email');
            if (emailEl) emailEl.textContent = user.email || '';
            const lgAvatar = document.getElementById('profile-avatar-lg');
            if (lgAvatar) {
                if (savedPhoto) {
                    lgAvatar.textContent = '';
                    lgAvatar.style.backgroundImage = `url(${savedPhoto})`;
                    lgAvatar.style.backgroundSize = 'cover';
                    lgAvatar.style.backgroundPosition = 'center';
                } else {
                    lgAvatar.style.backgroundImage = '';
                    lgAvatar.textContent = initial;
                }
            }
        }
        syncAdminBtnVisibility();
    }

    function applyTheme(theme) {
        const t = theme || 'system';
        const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
        const dark = t === 'dark' || (t === 'system' && prefersDark);
        document.documentElement.classList.toggle('dark-mode', dark);
        document.body?.classList.toggle('dark-mode', dark);
        document.documentElement.style.colorScheme = dark ? 'dark' : 'light';
        if (document.body) document.body.dataset.fdTheme = t;
    }

    function applyUserPrefs(prefs) {
        const p = prefs || getUserPrefs();
        applyTheme(p.theme || 'system');
        document.body.classList.remove('fd-density-cosy', 'fd-density-compact');
        if (p.density === 'cosy') document.body.classList.add('fd-density-cosy');
        if (p.density === 'compact') document.body.classList.add('fd-density-compact');
    }

    let themeMediaBound = false;
    function bindThemeMediaListener() {
        if (themeMediaBound) return;
        themeMediaBound = true;
        window.matchMedia?.('(prefers-color-scheme: dark)')?.addEventListener?.('change', () => {
            if ((getUserPrefs().theme || 'system') === 'system') applyUserPrefs();
        });
    }

    let settingsRenderGen = 0;

    function showSettingsShell(section) {
        const page = document.getElementById('settings-page');
        if (!page) return 0;
        document.getElementById('settings-menu')?.classList.add('hidden');
        document.getElementById('topbar-settings')?.setAttribute('aria-expanded', 'false');
        document.getElementById('profile-dropdown')?.classList.add('hidden');
        page.classList.remove('hidden');
        page.setAttribute('aria-hidden', 'false');
        document.body.classList.add('settings-open');
        const title = document.getElementById('settings-title');
        if (title) title.textContent = 'Settings';
        document.querySelectorAll('#settings-nav [data-section]').forEach((btn) => {
            const active = btn.getAttribute('data-section') === section;
            btn.classList.toggle('active', active);
            btn.setAttribute('aria-current', active ? 'page' : 'false');
        });
        return ++settingsRenderGen;
    }

    function closeSettingsPage() {
        settingsRenderGen += 1;
        const page = document.getElementById('settings-page');
        page?.classList.add('hidden');
        page?.setAttribute('aria-hidden', 'true');
        document.body.classList.remove('settings-open');
    }

    function openSettingsPage(section) {
        if (section === 'security') {
            openSecurityCenter();
            return;
        }
        if (section === 'account') {
            openDriveSettings();
            return;
        }
        if (section === 'shortcuts') {
            renderShortcutsSection();
            return;
        }
        if (section === 'privacy') {
            renderPrivacySection();
            return;
        }
        if (section === 'notifications') {
            renderNotificationsSection();
            return;
        }
        renderGeneralSection();
    }

    function soonBadge() {
        return '<span class="settings-soon">Soon</span>';
    }

    function settingsRow(title, desc, controlHtml, soon) {
        return `<div class="settings-row${soon ? ' settings-row--soon' : ''}">
            <div class="settings-row-copy">
                <div class="settings-row-title">${title}${soon ? soonBadge() : ''}</div>
                ${desc ? `<div class="settings-row-desc">${desc}</div>` : ''}
            </div>
            <div class="settings-row-control">${controlHtml}</div>
        </div>`;
    }

    function radioChoices(pref, current, options) {
        return options.map(([value, label]) => `
            <label class="settings-choice">
                <input type="radio" name="pref-${pref}" data-pref="${pref}" value="${value}" ${current === value ? 'checked' : ''}>
                <span>${label}</span>
            </label>`).join('');
    }

    function bindPrefRadios(root) {
        root.querySelectorAll('input[type="radio"][data-pref]').forEach((input) => {
            input.addEventListener('change', () => {
                if (!input.checked) return;
                const next = { ...getUserPrefs(), [input.dataset.pref]: input.value };
                setUserPrefs(next);
                applyUserPrefs(next);
            });
        });
    }

    function readSearchHistory() {
        try {
            const list = JSON.parse(localStorage.getItem('fd_search_history') || '[]');
            return Array.isArray(list) ? list.map((item) => String(item || '').trim()).filter(Boolean) : [];
        } catch {
            return [];
        }
    }

    function clearSearchHistory() {
        localStorage.removeItem('fd_search_history');
    }

    function renderGeneralSection() {
        const gen = showSettingsShell('general');
        const pane = document.getElementById('settings-pane');
        if (!pane || gen !== settingsRenderGen) return;
        const prefs = getUserPrefs();
        const start = prefs.startPage === '#/files' ? '#/files' : '#/home';
        const theme = prefs.theme === 'light' || prefs.theme === 'dark' ? prefs.theme : 'system';
        const density = prefs.density === 'cosy' || prefs.density === 'compact' ? prefs.density : 'comfortable';
        const pdfOpen = prefs.pdfOpen === 'tab' ? 'tab' : 'preview';
        pane.innerHTML = `<div class="settings-pane-pad">
            ${settingsRow('Start page', 'Open this page when you sign in', radioChoices('startPage', start, [['#/home', 'Home'], ['#/files', 'My Drive']]))}
            ${settingsRow('Appearance', '', radioChoices('theme', theme, [['light', 'Light'], ['dark', 'Dark'], ['system', 'Device default']]))}
            ${settingsRow('Density', 'Spacing of files and folders', radioChoices('density', density, [['comfortable', 'Comfortable'], ['cosy', 'Cosy'], ['compact', 'Compact']]))}
            ${settingsRow('Open PDFs', 'Preview opens the in-app viewer. New tab opens the file in a browser tab.', radioChoices('pdfOpen', pdfOpen, [['preview', 'Preview'], ['tab', 'New tab']]))}
            ${settingsRow('Convert uploads', 'Convert uploaded files to FreeDrive editor format', '<label class="settings-choice"><input type="checkbox" disabled><span>Convert uploads</span></label>', true)}
            ${settingsRow('Offline', 'Edit files without a connection', '<label class="settings-choice"><input type="checkbox" disabled><span>Offline editing</span></label>', true)}
            ${settingsRow('Preview cards', 'Show a preview card when hovering a file', '<label class="settings-choice"><input type="checkbox" disabled><span>Preview cards on hover</span></label>', true)}
            ${settingsRow('Sounds', 'Play sounds for uploads and shares', '<label class="settings-choice"><input type="checkbox" disabled><span>Sounds</span></label>', true)}
            ${settingsRow('Language', 'Display language', '<select disabled><option>English</option></select>', true)}
        </div>`;
        bindPrefRadios(pane);
    }

    function renderPrivacySection() {
        const gen = showSettingsShell('privacy');
        const pane = document.getElementById('settings-pane');
        if (!pane || gen !== settingsRenderGen) return;
        const keep = getUserPrefs().keepSearchHistory !== false;
        const history = readSearchHistory();
        const esc = Components.escapeHtml;
        const list = history.length
            ? `<ul class="settings-history-list">${history.map((q) => `<li>${esc(q)}</li>`).join('')}</ul>`
            : '<div class="settings-row-desc">No search history on this device.</div>';
        pane.innerHTML = `<div class="settings-pane-pad">
            ${settingsRow('Search history', 'Queries you submit from the search box, stored only in this browser.', `
                <label class="settings-choice">
                    <input type="checkbox" id="settings-no-search-history" ${keep ? '' : 'checked'}>
                    <span>Don&apos;t keep search history</span>
                </label>
                ${list}
                <button type="button" class="btn btn-secondary" id="settings-clear-search-history">Clear search history</button>
            `)}
            ${settingsRow('Workspace smart features', 'Manage how Workspace uses your data', '<button type="button" class="btn btn-secondary" disabled>Manage</button>', true)}
        </div>`;
        pane.querySelector('#settings-no-search-history')?.addEventListener('change', (e) => {
            const dontKeep = Boolean(e.target.checked);
            const next = { ...getUserPrefs(), keepSearchHistory: !dontKeep };
            setUserPrefs(next);
            if (dontKeep) clearSearchHistory();
            renderPrivacySection();
        });
        pane.querySelector('#settings-clear-search-history')?.addEventListener('click', () => {
            clearSearchHistory();
            Components.toast('Search history cleared', 'success');
            renderPrivacySection();
        });
    }

    function renderNotificationsSection() {
        const gen = showSettingsShell('notifications');
        const pane = document.getElementById('settings-pane');
        if (!pane || gen !== settingsRenderGen) return;
        const box = (label) => `<label class="settings-choice"><input type="checkbox" disabled><span>${label}</span></label>`;
        pane.innerHTML = `<div class="settings-pane-pad">
            <div class="settings-group-label">Browser</div>
            ${settingsRow('Browser notifications', 'Alerts in this browser', `${box('Comments and mentions')}${box('Items shared with me')}`, true)}
            <div class="settings-group-label">Email</div>
            ${settingsRow('Email notifications', 'Messages sent to your account email', `${box('Comments and suggestions')}${box('Requests for access')}${box('Items shared with me')}`, true)}
        </div>`;
    }

    function renderShortcutsSection() {
        showSettingsShell('shortcuts');
        const pane = document.getElementById('settings-pane');
        if (!pane) return;
        pane.innerHTML = '<div id="settings-shortcuts" class="shortcuts-grid settings-shortcuts settings-pane-pad"></div>';
        const target = document.getElementById('settings-shortcuts');
        if (target && typeof FileManager !== 'undefined' && FileManager.renderShortcutsInto) {
            FileManager.renderShortcutsInto(target);
        }
    }

    function formatEmailExpiry(iso) {
        if (!iso) return 'soon';
        try {
            const d = new Date(iso);
            if (Number.isNaN(d.getTime())) return 'soon';
            return d.toLocaleString();
        } catch {
            return 'soon';
        }
    }

    async function openDriveSettings() {
        const gen = showSettingsShell('account');
        const user = API.getUser() || {};
        const prefs = getUserPrefs();
        const esc = Components.escapeHtml;
        const currentName = user.username || '';
        const nameParts = currentName.split(' ');
        const firstName = nameParts[0] || '';
        const lastName = nameParts.slice(1).join(' ') || '';
        const previewAvatar = resolveAvatar(user, prefs);
        const currentEmail = String(user.email || '').trim().toLowerCase();

        let pendingStatus = { pending: false };
        try {
            pendingStatus = await API.emailChangeStatus();
        } catch { /* ignore */ }

        if (gen !== settingsRenderGen) return;

        const pendingBanner = pendingStatus.pending
            ? `<div id="settings-email-pending" class="fd-banner fd-banner-info">
                Check your inbox at <strong>${esc(pendingStatus.new_email_masked || 'your new address')}</strong>.
                The confirmation link expires ${esc(formatEmailExpiry(pendingStatus.expires_at))}.
            </div>`
            : '';

        const pane = document.getElementById('settings-pane');
        if (!pane) return;
        pane.innerHTML = `
            <div class="drive-settings-modal" style="padding: 8px 0;">
                <div class="drive-settings-profile" style="margin-bottom: 24px;">
                    <div class="drive-settings-avatar ${previewAvatar ? 'has-photo' : ''}" id="settings-avatar-preview" style="${previewAvatar ? `background-image:url(${previewAvatar});` : ''}">${previewAvatar ? '' : esc(Components.initials(user.username || user.email || 'U'))}</div>
                    <div class="drive-settings-meta">
                        <div class="drive-settings-name">${esc(user.username || 'User')}</div>
                        <div class="drive-settings-email">${esc(user.email || 'No email')}</div>
                        <div class="drive-settings-avatar-actions">
                            <label class="drive-avatar-upload-btn" for="settings-avatar-input">Upload photo</label>
                            <button type="button" class="drive-avatar-remove-btn" id="settings-avatar-remove">Remove</button>
                        </div>
                    </div>
                </div>
                <div style="display:flex;flex-direction:column;gap:16px;">
                    <label class="drive-settings-field drive-settings-field-full">
                        <span class="fd-label">First name</span>
                        <input id="settings-first-name" class="fd-input" type="text" value="${esc(firstName)}" placeholder="First name">
                    </label>
                    <label class="drive-settings-field drive-settings-field-full">
                        <span class="fd-label">Last name</span>
                        <input id="settings-last-name" class="fd-input" type="text" value="${esc(lastName)}" placeholder="Last name">
                    </label>
                    <label class="drive-settings-field drive-settings-field-full">
                        <span class="fd-label">Email</span>
                        <input id="settings-email" class="fd-input" type="email" value="${esc(user.email || '')}" placeholder="Email" autocomplete="email">
                        <span class="fd-hint">Confirmation link will be sent to the new address.</span>
                    </label>
                    <label class="drive-settings-field drive-settings-field-full hidden" id="settings-email-password-wrap">
                        <span class="fd-label">Current password</span>
                        <input id="settings-email-password" class="fd-input" type="password" placeholder="Required to change email" autocomplete="current-password">
                    </label>
                    <button type="button" class="btn btn-secondary drive-settings-confirm-btn" id="settings-send-email-confirm">Confirm</button>
                    ${pendingBanner}
                </div>
                <div class="settings-actions">
                    <button type="button" class="btn btn-primary" id="settings-save-profile">Save</button>
                </div>
                <input id="settings-avatar-input" type="file" accept="image/*" hidden>
            </div>
        `;

        const fileInput = document.getElementById('settings-avatar-input');
        const avatarPreview = document.getElementById('settings-avatar-preview');
        const removeBtn = document.getElementById('settings-avatar-remove');
        if (avatarPreview) avatarPreview.dataset.avatar = previewAvatar || '';

        fileInput?.addEventListener('change', async () => {
            const file = fileInput.files?.[0];
            if (!file) return;
            const reader = new FileReader();
            reader.onload = async () => {
                const raw = String(reader.result || '');
                const result = await resizeAvatarDataURL(raw);
                if (!avatarPreview) return;
                avatarPreview.dataset.avatar = result;
                avatarPreview.textContent = '';
                avatarPreview.style.backgroundImage = `url(${result})`;
                avatarPreview.classList.add('has-photo');
            };
            reader.readAsDataURL(file);
        });

        removeBtn?.addEventListener('click', () => {
            if (!avatarPreview) return;
            avatarPreview.dataset.avatar = '';
            avatarPreview.textContent = esc(Components.initials(user.username || user.email || 'U'));
            avatarPreview.style.backgroundImage = '';
            avatarPreview.classList.remove('has-photo');
            if (fileInput) fileInput.value = '';
        });

        const emailInput = document.getElementById('settings-email');
        const passwordWrap = document.getElementById('settings-email-password-wrap');
        const passwordInput = document.getElementById('settings-email-password');
        const sendConfirmBtn = document.getElementById('settings-send-email-confirm');

        function syncEmailPasswordVisibility() {
            const nextEmail = String(emailInput?.value || '').trim().toLowerCase();
            const changing = Boolean(nextEmail && nextEmail !== currentEmail);
            passwordWrap?.classList.toggle('hidden', !changing);
            if (!changing && passwordInput) passwordInput.value = '';
        }

        emailInput?.addEventListener('input', syncEmailPasswordVisibility);
        syncEmailPasswordVisibility();

        sendConfirmBtn?.addEventListener('click', async () => {
            const newEmail = String(emailInput?.value || '').trim().toLowerCase();
            const password = String(passwordInput?.value || '');
            if (!newEmail || !newEmail.includes('@')) {
                Components.toast('Enter a valid email address', 'error');
                return;
            }
            if (newEmail === currentEmail) {
                Components.toast('Enter a different email address', 'error');
                return;
            }
            if (!password) {
                Components.toast('Current password is required to change email', 'error');
                return;
            }

            sendConfirmBtn.disabled = true;
            const prevLabel = sendConfirmBtn.textContent;
            sendConfirmBtn.textContent = 'Sending...';
            try {
                const result = await API.requestEmailChange(newEmail, password);
                Components.toast(`Confirmation link sent to ${result.new_email_masked || newEmail}`, 'success', { duration: 7000 });
                if (passwordInput) passwordInput.value = '';
                openDriveSettings();
            } catch (err) {
                Components.toast(err?.message || 'Failed to request email change', 'error');
            } finally {
                sendConfirmBtn.disabled = false;
                sendConfirmBtn.textContent = prevLabel;
            }
        });

        document.getElementById('settings-save-profile')?.addEventListener('click', async () => {
            const first = String(document.getElementById('settings-first-name')?.value || '').trim();
            const last = String(document.getElementById('settings-last-name')?.value || '').trim();
            const fullName = [first, last].filter(Boolean).join(' ');
            if (!fullName) {
                Components.toast('First or last name is required', 'error');
                return;
            }

            const preview = document.getElementById('settings-avatar-preview');
            const avatarRaw = preview && Object.prototype.hasOwnProperty.call(preview.dataset, 'avatar')
                ? String(preview.dataset.avatar || '')
                : resolveAvatar(user, prefs);

            const saveBtn = document.getElementById('settings-save-profile');
            if (saveBtn) saveBtn.disabled = true;
            try {
                const avatar = avatarRaw
                    ? await resizeAvatarDataURL(avatarRaw)
                    : '';
                const updated = await API.updateMe({
                    username: fullName,
                    avatar_url: avatar,
                });
                API.setUser(updated);
                syncAvatarCache(updated.avatar_url || '');
                refreshUserUI();
                Components.toast('Profile updated', 'success');
            } catch (err) {
                Components.toast(err?.message || 'Failed to save profile', 'error');
            } finally {
                if (saveBtn) saveBtn.disabled = false;
            }
        });
    }

    async function openSecurityCenter() {
        const gen = showSettingsShell('security');
        const esc = Components.escapeHtml;
        let profile = API.getUser() || {};
        try {
            profile = await API.me();
            API.setUser(profile);
        } catch { /* use cached */ }

        const required = Boolean(profile.two_factor_required);
        const emailEnabled = Boolean(profile.email_2fa_enabled);
        const totpEnabled = Boolean(profile.totp_enabled);
        const phoneEnabled = Boolean(profile.login_approval_enabled);
        let phoneStatus = { enabled: phoneEnabled, has_trusted_mobile: false };
        try {
            phoneStatus = await API.loginApprovalStatus();
        } catch { /* ignore */ }

        const requiredNote = required
            ? '<p class="fd-hint fd-status-info" style="margin:12px 0 0;">Your administrator requires two-factor authentication (authenticator app or email).</p>'
            : '';

        let needsRecovery = false;
        if (window.CryptoSync?.detectNeedsRecovery) {
            try {
                needsRecovery = await CryptoSync.detectNeedsRecovery();
            } catch { /* ignore */ }
        }
        const recoveryBanner = needsRecovery
            ? `<div id="security-crypto-recovery-banner" class="fd-banner fd-banner-danger" style="margin-bottom:12px;margin-top:0;">
                Server lost encryption account data, but encrypted file keys are still on the server. Enter your recovery code below to restore access.
            </div>`
            : '';
        const recoverySection = needsRecovery
            ? `<div id="security-crypto-recovery-section" style="margin-bottom:12px;">
                <input id="security-crypto-recovery-input" class="fd-input" type="text" placeholder="xxxx-xxxx-..." style="height:40px;">
                <button type="button" class="btn btn-secondary" id="security-crypto-recovery-restore-btn" style="margin-top:8px;">
                    Restore encryption
                </button>
            </div>`
            : '';

        const totpBody = totpEnabled
            ? `<p class="fd-muted fd-status-ok" style="margin:0;">Authenticator app is enabled${profile.totp_enrolled_at ? ` (since ${esc(new Date(profile.totp_enrolled_at).toLocaleDateString())})` : ''}.</p>
               <button type="button" class="btn btn-secondary" id="security-totp-disable-btn" style="margin-top:12px;">Disable authenticator</button>`
            : `<p class="fd-muted" style="margin:0 0 12px;">Use Google Authenticator, Authy, or 1Password for sign-in codes. Preferred when both methods are enabled.</p>
               <button type="button" class="btn btn-primary" id="security-totp-setup-btn">Set up authenticator</button>
               <div id="security-totp-setup-panel" class="hidden" style="margin-top:14px;"></div>`;

        const emailCanDisable = !(required && !totpEnabled);
        const emailBody = emailEnabled
            ? `<p class="fd-muted fd-status-ok" style="margin:0;">Email verification codes are enabled for ${esc(profile.email || 'your email')}.</p>
               <button type="button" class="btn btn-secondary" id="security-email-2fa-disable-btn" style="margin-top:12px;" ${emailCanDisable ? '' : 'disabled'}>Disable email codes</button>
               ${emailCanDisable ? '' : '<p class="fd-hint fd-status-info" style="margin:8px 0 0;">Required by your administrator until an authenticator app is set up.</p>'}`
            : `<p class="fd-muted" style="margin:0 0 12px;">Protect your account with a 6-digit code sent to ${esc(profile.email || 'your email')} each time you sign in.</p>
               <button type="button" class="btn btn-primary" id="security-email-2fa-enable-btn">Enable email codes</button>
               ${requiredNote}`;

        const phoneTrustedNote = phoneStatus.has_trusted_mobile
            ? '<p class="fd-hint fd-status-ok" style="margin:8px 0 0;">Trusted FreeDrive mobile app detected.</p>'
            : '<p class="fd-hint fd-status-warn" style="margin:8px 0 0;">No trusted mobile app signed in yet. Install FreeDrive on your phone and stay signed in.</p>';
        const phoneBody = phoneEnabled
            ? `<p class="fd-muted fd-status-ok" style="margin:0;">Phone sign-in prompts are enabled.</p>
               ${phoneTrustedNote}
               <button type="button" class="btn btn-secondary" id="security-phone-approval-disable-btn" style="margin-top:12px;">Disable phone prompts</button>`
            : `<p class="fd-muted" style="margin:0 0 12px;">When you sign in on a new computer or browser, FreeDrive can ask your phone to approve instead of an authenticator code. Requires the FreeDrive mobile app signed in.</p>
               <button type="button" class="btn btn-primary" id="security-phone-approval-enable-btn">Enable phone prompts</button>
               ${phoneTrustedNote}`;

        if (gen !== settingsRenderGen) return;
        const pane = document.getElementById('settings-pane');
        if (!pane) return;
        pane.innerHTML = `
            <div class="drive-settings-modal" style="padding:8px 0;display:flex;flex-direction:column;gap:16px;">
                <div class="fd-card">
                    <div class="fd-card-title">Authenticator app</div>
                    <div class="fd-card-desc">Protect your account with a time-based code from an authenticator app.</div>
                    ${totpBody}
                </div>
                <div class="fd-card">
                    <div class="fd-card-title">Email two-factor authentication</div>
                    <div class="fd-card-desc">Protect your account with a verification code sent by email.</div>
                    ${emailBody}
                </div>
                <div class="fd-card">
                    <div class="fd-card-title">Phone sign-in prompts</div>
                    <div class="fd-card-desc">Approve sign-ins from your phone, like Google Prompt.</div>
                    ${phoneBody}
                </div>
                <div class="fd-card">
                    <div style="display:flex;align-items:center;justify-content:space-between;gap:12px;margin-bottom:8px;">
                        <div class="fd-card-title" style="margin:0;">Devices</div>
                        <button type="button" class="btn btn-secondary" id="security-revoke-others-btn" style="font-size:12px;height:32px;">
                            Sign out other devices
                        </button>
                    </div>
                    <p class="fd-hint" style="margin:0 0 12px;">
                        Devices currently signed in to your account. Signing out a device forces it to log in again.
                    </p>
                    <div id="security-sessions-list" style="display:flex;flex-direction:column;gap:8px;">
                        <div class="fd-muted">Loading devices…</div>
                    </div>
                </div>
                <div class="fd-card">
                    <div class="fd-card-title" style="margin-bottom:8px;">Encryption</div>
                    ${recoveryBanner}
                    <p class="fd-hint" style="margin:0 0 8px;">
                        Status: <strong id="security-crypto-status">${window.CryptoSync?.isUnlocked?.() ? 'Active' : 'Inactive'}</strong>
                        — encryption unlocks automatically when you sign in. Keys sync across your devices.
                    </p>
                    ${recoverySection}
                    <details class="fd-muted" style="margin-bottom:12px;">
                        <summary style="cursor:pointer;">Advanced: rotate encryption key</summary>
                        <button type="button" class="btn btn-secondary" id="security-crypto-rotate-btn" style="margin-top:8px;">
                            Rotate encryption key
                        </button>
                    </details>
                    <div class="fd-label">Manual backup (optional)</div>
                    <p class="fd-hint" style="margin:0 0 12px;">
                        Export/import below only if you need to move keys manually between browsers.
                    </p>
                    <div style="display:flex;flex-wrap:wrap;gap:8px;">
                        <button type="button" class="btn btn-secondary" id="security-export-keys-btn">Export encryption keys</button>
                        <button type="button" class="btn btn-secondary" id="security-import-keys-btn">Import encryption keys</button>
                    </div>
                    <input type="file" id="security-import-keys-input" accept="application/json,.json" hidden>
                </div>
            </div>
        `;
        if (gen !== settingsRenderGen) return;

        const setEmail2FA = async (next) => {
            if (required && !totpEnabled && !next) {
                Components.toast('Two-factor authentication is required by your administrator', 'info');
                return;
            }
            try {
                const updated = await API.updateMe({ email_2fa_enabled: next });
                API.setUser(updated);
                Components.toast(next ? 'Email verification codes enabled' : 'Email verification codes disabled', 'success');
                openSecurityCenter();
            } catch (err) {
                Components.toast(err?.message || 'Failed to update email two-factor setting', 'error');
            }
        };
        document.getElementById('security-email-2fa-enable-btn')?.addEventListener('click', () => setEmail2FA(true));
        document.getElementById('security-email-2fa-disable-btn')?.addEventListener('click', () => setEmail2FA(false));

        const setPhoneApproval = async (next) => {
            try {
                const updated = await API.updateMe({ login_approval_enabled: next });
                API.setUser(updated);
                Components.toast(next ? 'Phone sign-in prompts enabled' : 'Phone sign-in prompts disabled', 'success');
                openSecurityCenter();
            } catch (err) {
                Components.toast(err?.message || 'Failed to update phone prompt setting', 'error');
            }
        };
        document.getElementById('security-phone-approval-enable-btn')?.addEventListener('click', () => setPhoneApproval(true));
        document.getElementById('security-phone-approval-disable-btn')?.addEventListener('click', () => setPhoneApproval(false));

        const showBackupCodes = (codes) => {
            const list = (codes || []).map((c) => `<code class="fd-code-block" style="display:inline-block;margin:2px 6px 2px 0;padding:4px 8px;">${esc(c)}</code>`).join('');
            Components.showModal('Backup codes', `
                <p class="fd-muted" style="margin:0 0 12px;">
                    Save these one-time backup codes somewhere safe. Each code can be used once if you lose access to your authenticator.
                </p>
                <div style="line-height:1.8;">${list || 'No codes returned.'}</div>
            `, [{ text: 'Done', class: 'btn-primary', action: () => openSecurityCenter() }]);
        };

        document.getElementById('security-totp-setup-btn')?.addEventListener('click', async () => {
            const panel = document.getElementById('security-totp-setup-panel');
            const btn = document.getElementById('security-totp-setup-btn');
            if (!panel) return;
            try {
                if (btn) btn.disabled = true;
                const setup = await API.totpSetup();
                panel.classList.remove('hidden');
                panel.innerHTML = `
                    <div style="display:flex;flex-wrap:wrap;gap:16px;align-items:flex-start;">
                        <img src="${esc(setup.qr)}" alt="Authenticator QR code" width="160" height="160" style="border:1px solid var(--fd-border);border-radius:8px;">
                        <div style="flex:1;min-width:180px;">
                            <p class="fd-muted" style="margin:0 0 8px;">Scan this QR code, or enter the secret manually:</p>
                            <code class="fd-code-block" style="display:block;">${esc(setup.secret)}</code>
                            <input id="security-totp-confirm-code" class="fd-input" type="text" inputmode="numeric" maxlength="8" placeholder="6-digit code" style="height:40px;margin-top:12px;">
                            <button type="button" class="btn btn-primary" id="security-totp-confirm-btn" style="margin-top:8px;">Confirm</button>
                        </div>
                    </div>`;
                document.getElementById('security-totp-confirm-btn')?.addEventListener('click', async () => {
                    const code = String(document.getElementById('security-totp-confirm-code')?.value || '').trim();
                    if (code.length < 6) {
                        Components.toast('Enter the 6-digit code from your authenticator', 'error');
                        return;
                    }
                    try {
                        const result = await API.totpConfirm(code);
                        const updated = await API.me();
                        API.setUser(updated);
                        Components.toast('Authenticator app enabled', 'success');
                        showBackupCodes(result?.backup_codes || []);
                    } catch (err) {
                        Components.toast(err?.message || 'Failed to confirm authenticator', 'error');
                    }
                });
            } catch (err) {
                Components.toast(err?.message || 'Failed to start authenticator setup', 'error');
            } finally {
                if (btn) btn.disabled = false;
            }
        });

        document.getElementById('security-totp-disable-btn')?.addEventListener('click', async () => {
            const code = window.prompt('Enter a current authenticator code (or leave blank and enter password next):') || '';
            let password = '';
            if (!String(code).trim()) {
                password = window.prompt('Enter your account password to disable authenticator:') || '';
            }
            if (!String(code).trim() && !String(password).trim()) return;
            try {
                const updated = await API.totpDisable({ code: String(code).trim(), password: String(password) });
                if (updated?.id) API.setUser(updated);
                else {
                    const me = await API.me();
                    API.setUser(me);
                }
                Components.toast('Authenticator app disabled', 'success');
                openSecurityCenter();
            } catch (err) {
                Components.toast(err?.message || 'Failed to disable authenticator', 'error');
            }
        });

        const sessionsListEl = document.getElementById('security-sessions-list');
        const formatSessionTime = (iso) => {
            if (!iso) return 'Unknown';
            const ts = new Date(iso).getTime();
            if (Number.isNaN(ts)) return 'Unknown';
            const mins = Math.floor((Date.now() - ts) / 60000);
            if (mins < 1) return 'Just now';
            if (mins < 60) return `${mins} min ago`;
            const hours = Math.floor(mins / 60);
            if (hours < 24) return `${hours}h ago`;
            const days = Math.floor(hours / 24);
            if (days < 14) return `${days}d ago`;
            return new Date(iso).toLocaleDateString();
        };

        const renderSessions = async () => {
            if (!sessionsListEl) return;
            sessionsListEl.innerHTML = '<div class="fd-muted">Loading devices…</div>';
            try {
                const data = await API.auth.getSessions();
                const sessions = Array.isArray(data?.sessions) ? data.sessions : [];
                if (!sessions.length) {
                    sessionsListEl.innerHTML = '<div class="fd-muted">No active devices.</div>';
                    return;
                }
                sessionsListEl.innerHTML = sessions.map((s) => {
                    const type = String(s.device_type || '').toLowerCase();
                    let icon;
                    if (type === 'mobile') {
                        // smartphone
                        icon = '<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M17 1.01 7 1c-1.1 0-2 .9-2 2v18c0 1.1.9 2 2 2h10c1.1 0 2-.9 2-2V3c0-1.1-.9-1.99-2-1.99zM17 19H7V5h10v14z"/></svg>';
                    } else if (type === 'desktop') {
                        // monitor
                        icon = '<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M21 2H3c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h7v2H8v2h8v-2h-2v-2h7c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm0 14H3V4h18v12z"/></svg>';
                    } else {
                        // web / browser — globe
                        icon = '<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z"/></svg>';
                    }
                    const badge = s.current
                        ? '<span class="fd-badge-current">This device</span>'
                        : '';
                    const revokeBtn = s.current
                        ? ''
                        : `<button type="button" class="btn btn-secondary security-revoke-session-btn" data-session-id="${esc(s.id)}" style="font-size:12px;height:30px;">Sign out</button>`;
                    return `<div class="fd-device-row">
                        <div style="display:flex;align-items:flex-start;gap:12px;min-width:0;">
                            <div class="fd-muted" style="margin-top:2px;">${icon}</div>
                            <div style="min-width:0;">
                                <div style="font-size:14px;font-weight:500;color:var(--fd-text);">${esc(s.device_name || 'Unknown device')}${badge}</div>
                                <div class="fd-hint" style="margin-top:2px;">${esc(s.ip_address || '—')} · Last active ${esc(formatSessionTime(s.last_seen_at))}</div>
                            </div>
                        </div>
                        ${revokeBtn}
                    </div>`;
                }).join('');

                sessionsListEl.querySelectorAll('.security-revoke-session-btn').forEach((btn) => {
                    btn.addEventListener('click', async () => {
                        const id = btn.getAttribute('data-session-id');
                        if (!id) return;
                        if (!window.confirm('Sign out this device? It will need to log in again.')) return;
                        btn.disabled = true;
                        try {
                            await API.auth.revokeSession(id);
                            Components.toast('Device signed out', 'success');
                            await renderSessions();
                        } catch (err) {
                            btn.disabled = false;
                            Components.toast(err?.message || 'Failed to sign out device', 'error');
                        }
                    });
                });
            } catch (err) {
                sessionsListEl.innerHTML = `<div class="fd-muted" style="color:var(--fd-red);">${esc(err?.message || 'Failed to load devices')}</div>`;
            }
        };

        document.getElementById('security-revoke-others-btn')?.addEventListener('click', async () => {
            if (!window.confirm('Sign out all other devices? They will need to log in again.')) return;
            const btn = document.getElementById('security-revoke-others-btn');
            if (btn) btn.disabled = true;
            try {
                await API.auth.revokeOtherSessions();
                Components.toast('Other devices signed out', 'success');
                await renderSessions();
            } catch (err) {
                Components.toast(err?.message || 'Failed to sign out other devices', 'error');
            } finally {
                if (btn) btn.disabled = false;
            }
        });

        renderSessions();

        document.getElementById('security-export-keys-btn')?.addEventListener('click', async () => {
            try {
                if (!CryptoModule.canEncrypt()) {
                    Components.toast('Encryption is not available in this browser session', 'error');
                    return;
                }
                const exportData = await CryptoModule.exportAllKeys();
                const count = Object.keys(exportData.keys || {}).length;
                if (count === 0) {
                    Components.toast('No encryption keys found in this browser', 'info');
                    return;
                }
                CryptoModule.downloadKeyExport(exportData);
                Components.toast(`Exported ${count} encryption key${count === 1 ? '' : 's'}`, 'success');
            } catch (err) {
                Components.toast(err?.message || 'Failed to export encryption keys', 'error');
            }
        });

        const importKeysInput = document.getElementById('security-import-keys-input');
        document.getElementById('security-import-keys-btn')?.addEventListener('click', () => {
            importKeysInput?.click();
        });
        importKeysInput?.addEventListener('change', async () => {
            const file = importKeysInput.files?.[0];
            importKeysInput.value = '';
            if (!file) return;
            try {
                if (!CryptoModule.canEncrypt()) {
                    Components.toast('Encryption is not available in this browser session', 'error');
                    return;
                }
                const text = await file.text();
                const exportData = CryptoModule.parseKeyExportFile(text);
                const count = await CryptoModule.importAllKeys(exportData);
                if (count === 0) {
                    Components.toast('No valid keys found in import file', 'info');
                    return;
                }
                Components.toast(`Imported ${count} encryption key${count === 1 ? '' : 's'}`, 'success');
            } catch (err) {
                Components.toast(err?.message || 'Failed to import encryption keys', 'error');
            }
        });

        document.getElementById('security-crypto-recovery-restore-btn')?.addEventListener('click', async () => {
            const recovery = String(document.getElementById('security-crypto-recovery-input')?.value || '').trim();
            if (!recovery) {
                Components.toast('Enter your recovery code', 'error');
                return;
            }
            try {
                await CryptoSync.restoreWithRecoveryCode(recovery);
                const statusEl = document.getElementById('security-crypto-status');
                if (statusEl) statusEl.textContent = 'Active';
                document.getElementById('security-crypto-recovery-banner')?.remove();
                document.getElementById('security-crypto-recovery-section')?.remove();
                Components.toast('Encryption restored', 'success');
            } catch (err) {
                Components.toast(err?.message || 'Recovery failed', 'error');
            }
        });

        document.getElementById('security-crypto-rotate-btn')?.addEventListener('click', async () => {
            if (!window.CryptoSync?.rotateAccountKey) return;
            const password = window.prompt('Enter your password to rotate your encryption key:');
            if (!password) return;
            try {
                const result = await CryptoSync.rotateAccountKey(password);
                if (result?.recoveryCode && CryptoSync.showRecoverySetupModal) {
                    await CryptoSync.showRecoverySetupModal(result.recoveryCode);
                }
                const statusEl = document.getElementById('security-crypto-status');
                if (statusEl) statusEl.textContent = 'Active';
                Components.toast('Encryption key rotated', 'success');
            } catch (err) {
                Components.toast(err?.message || 'Key rotation failed', 'error');
            }
        });
    }

    async function init() {
        applyUserPrefs();
        bindThemeMediaListener();

        if (window.CryptoModule?.migrateUnscopedFileKeys) {
            try { await CryptoModule.migrateUnscopedFileKeys(); } catch { /* ignore */ }
        }
        if (!window.location.hash && window.location.pathname.startsWith('/admin') && window.location.pathname !== '/admin') {
            // We use history wrapper below, do not convert to hash.
            // Allow pathname to dictate routing.
        }

        Components.init();
        Auth.init();
        Upload.init();
        FileManager.init();
        if (API.isLoggedIn()) SidebarTree.init();

        if (API.isLoggedIn()) {
            const user = API.getUser();
            if (user?.must_change_password) {
                Auth.showForcePasswordForm();
            } else if (API.isAddingAccount?.()) {
                showAuth();
                Auth.syncAddingAccountUi?.();
            } else {
                showApp();
            }
        } else {
            showAuth();
        }

        bindGlobalUI();
        window.addEventListener('hashchange', handleRoute);
        window.addEventListener('popstate', handleRoute);
        
        // Attach ripple effect to interactive elements
        initRipple();
        
        handleRoute();
    }

    function bindGlobalUI() {
        const newBtn = document.getElementById('new-menu-btn');
        const newDropdown = document.getElementById('new-dropdown');
        const helpDropdown = document.getElementById('help-dropdown');
        const searchFilterPanel = document.getElementById('search-filter-panel');
        const sidebar = document.getElementById('sidebar');
        const sidebarResizer = document.getElementById('sidebar-resizer');
        const detailsPanel = document.getElementById('details-panel');
        const notificationsPanel = document.getElementById('notifications-panel');
        const contentArea = document.getElementById('content-area');

        const closeTransientPanels = () => {
            newDropdown?.classList.add('hidden');
            helpDropdown?.classList.add('hidden');
            searchFilterPanel?.classList.add('hidden');
            document.getElementById('profile-dropdown')?.classList.add('hidden');
            document.getElementById('settings-menu')?.classList.add('hidden');
            document.getElementById('topbar-settings')?.setAttribute('aria-expanded', 'false');
        };
        const closeRightPanels = () => {
            FileManager.hideDetailsPanel();
            notificationsPanel?.classList.add('hidden');
        };

        const clampSidebarWidth = (rawWidth) => {
            const min = 220;
            const max = Math.max(min, Math.min(420, window.innerWidth - 420));
            return Math.min(max, Math.max(min, rawWidth));
        };

        const applySidebarWidth = (rawWidth) => {
            const clamped = clampSidebarWidth(rawWidth);
            document.documentElement.style.setProperty('--fd-sidebar-w', `${clamped}px`);
            return clamped;
        };

        const restoreSidebarWidth = () => {
            const saved = Number(localStorage.getItem('fd_sidebar_w') || 0);
            if (Number.isFinite(saved) && saved > 0) {
                applySidebarWidth(saved);
            }
        };

        const bindSidebarResizer = () => {
            if (!sidebar || !sidebarResizer) return;

            restoreSidebarWidth();
            let isResizing = false;
            let pointerId = null;
            let startX = 0;
            let startWidth = 0;

            const stopResize = (e) => {
                if (!isResizing) return;
                if (e && pointerId !== null && e.pointerId !== pointerId) return;

                isResizing = false;
                pointerId = null;
                document.body.classList.remove('is-resizing-sidebar');
                document.removeEventListener('pointermove', onPointerMove);
                document.removeEventListener('pointerup', stopResize);
                document.removeEventListener('pointercancel', stopResize);

                const current = parseInt(getComputedStyle(document.documentElement).getPropertyValue('--fd-sidebar-w'), 10);
                if (Number.isFinite(current) && current > 0) {
                    localStorage.setItem('fd_sidebar_w', String(current));
                }
            };

            const onPointerMove = (e) => {
                if (!isResizing || e.pointerId !== pointerId) return;
                e.preventDefault();
                const nextWidth = startWidth + (e.clientX - startX);
                applySidebarWidth(nextWidth);
            };

            sidebarResizer.addEventListener('pointerdown', (e) => {
                if (e.button !== 0 || window.matchMedia('(max-width: 1100px)').matches) return;
                e.preventDefault();
                isResizing = true;
                pointerId = e.pointerId;
                startX = e.clientX;
                startWidth = sidebar.getBoundingClientRect().width;
                document.body.classList.add('is-resizing-sidebar');
                document.addEventListener('pointermove', onPointerMove);
                document.addEventListener('pointerup', stopResize);
                document.addEventListener('pointercancel', stopResize);
            });

            window.addEventListener('resize', () => {
                if (window.matchMedia('(max-width: 1100px)').matches) return;
                const current = parseInt(getComputedStyle(document.documentElement).getPropertyValue('--fd-sidebar-w'), 10);
                if (Number.isFinite(current)) {
                    applySidebarWidth(current);
                }
            });
        };

        bindSidebarResizer();

        const closeMobileSidebar = () => {
            sidebar?.classList.remove('open');
        };

        document.getElementById('sidebar-toggle')?.addEventListener('click', (e) => {
            e.stopPropagation();
            sidebar?.classList.toggle('open');
        });

        document.getElementById('sidebar-scrim')?.addEventListener('click', () => {
            closeMobileSidebar();
        });

        sidebar?.addEventListener('click', (e) => {
            if (!window.matchMedia('(max-width: 1100px)').matches) return;
            const target = e.target;
            if (!target) return;
            if (!target.closest('.nav-item, .context-item')) return;
            closeMobileSidebar();
        });

        const settingsMenu = document.getElementById('settings-menu');
        const settingsBtn = document.getElementById('topbar-settings');
        settingsBtn?.addEventListener('click', (e) => {
            e.stopPropagation();
            const opening = settingsMenu?.classList.contains('hidden');
            closeTransientPanels();
            if (!opening) return;
            settingsMenu?.classList.remove('hidden');
            settingsBtn.setAttribute('aria-expanded', 'true');
        });
        settingsMenu?.addEventListener('click', (e) => {
            e.stopPropagation();
            const adminItem = e.target.closest('[data-admin-panel]');
            if (adminItem) {
                closeTransientPanels();
                history.pushState(null, '', '/admin/dashboard');
                handleRoute();
                return;
            }
            const item = e.target.closest('[data-section]');
            if (!item) return;
            closeTransientPanels();
            openSettingsPage(item.getAttribute('data-section') || 'account');
        });
        document.getElementById('settings-back')?.addEventListener('click', () => closeSettingsPage());
        document.getElementById('settings-nav')?.addEventListener('click', (e) => {
            const item = e.target.closest('[data-section]');
            if (!item) return;
            openSettingsPage(item.getAttribute('data-section') || 'account');
        });

        const shouldIgnorePanelDismiss = (target) => {
            if (!target) return true;
            if (target.closest('#details-panel, #notifications-panel, #notifications-btn, #info-btn')) return true;
            if (target.closest('.file-row, .file-card')) return true;
            if (target.closest('.modal-overlay:not(.hidden), #editor-overlay:not(.hidden), #context-menu')) return true;
            if (target.closest('#new-dropdown, #help-dropdown, .sidebar, #profile-dropdown, #topbar-profile-wrap, .search-wrap')) return true;
            return false;
        };

        document.addEventListener('click', (e) => {
            const notifOpen = notificationsPanel && !notificationsPanel.classList.contains('hidden');
            if (!notifOpen) return;
            if (shouldIgnorePanelDismiss(e.target)) return;
            notificationsPanel.classList.add('hidden');
        });

        newBtn?.addEventListener('click', (e) => {
            e.stopPropagation();
            const shouldOpen = newDropdown?.classList.contains('hidden');
            closeTransientPanels();
            if (!shouldOpen) return;
            newDropdown?.style.setProperty('position', 'absolute');
            newDropdown?.style.setProperty('left', '8px');
            newDropdown?.style.setProperty('top', '68px');
            newDropdown?.classList.remove('hidden');
        });

        newDropdown?.addEventListener('click', (e) => {
            e.stopPropagation();
        });

        document.addEventListener('click', () => {
            closeTransientPanels();
        });

        document.addEventListener('click', (e) => {
            if (!window.matchMedia('(max-width: 1100px)').matches) return;
            const target = e.target;
            if (target && (target.closest('#sidebar') || target.closest('#sidebar-toggle') || target.closest('#sidebar-scrim'))) return;
            closeMobileSidebar();
        });

        document.getElementById('new-folder-action')?.addEventListener('click', () => FileManager.createFolder());
        document.getElementById('new-doc-action')?.addEventListener('click', () => FileManager.createQuickFile('Document.txt', 'text/plain', ''));
        document.getElementById('new-sheet-action')?.addEventListener('click', () => FileManager.createQuickFile('Spreadsheet.csv', 'text/csv', 'Column 1,Column 2\n,\n'));
        document.getElementById('new-presentation-action')?.addEventListener('click', () => FileManager.createQuickFile('Presentation.md', 'text/markdown', '# New Presentation\n'));

        document.getElementById('file-upload-action')?.addEventListener('click', () => document.getElementById('file-input')?.click());
        document.getElementById('folder-upload-action')?.addEventListener('click', () => document.getElementById('folder-input')?.click());

        document.getElementById('file-input')?.addEventListener('change', (e) => {
            const files = e.target.files;
            if (files?.length) Upload.handleFiles(files);
            e.target.value = '';
        });

        document.getElementById('folder-input')?.addEventListener('change', (e) => {
            const files = e.target.files;
            if (files?.length) Upload.handleFolderFiles(files);
            e.target.value = '';
        });

        const main = document.querySelector('.main-content');
        if (main) {
            let dragCounter = 0;
            main.addEventListener('dragenter', (e) => {
                e.preventDefault();
                if (!FileManager.canAcceptUploads()) return;
                dragCounter += 1;
                document.getElementById('drop-overlay')?.classList.remove('hidden');
            });
            main.addEventListener('dragleave', (e) => {
                e.preventDefault();
                dragCounter -= 1;
                if (dragCounter <= 0) {
                    dragCounter = 0;
                    document.getElementById('drop-overlay')?.classList.add('hidden');
                }
            });
            main.addEventListener('dragover', (e) => e.preventDefault());
            main.addEventListener('drop', async (e) => {
                e.preventDefault();
                dragCounter = 0;
                document.getElementById('drop-overlay')?.classList.add('hidden');
                if (!FileManager.canAcceptUploads()) {
                    Components.toast('Connect a computer to upload files here', 'info');
                    return;
                }
                try {
                    const files = await Upload.collectFromDataTransfer(e.dataTransfer);
                    if (files.length) Upload.uploadFileTree(files);
                } catch {
                    Components.toast('Could not read dropped files', 'error');
                }
            });
        }

        document.querySelector('.search-wrap')?.addEventListener('click', (e) => {
            e.stopPropagation();
        });

        document.getElementById('search-options')?.addEventListener('click', (e) => {
            e.stopPropagation();
            newDropdown?.classList.add('hidden');
            helpDropdown?.classList.add('hidden');
            FileManager.hideSearchDropdown();
            searchFilterPanel?.classList.toggle('hidden');
            if (!searchFilterPanel?.classList.contains('hidden')) {
                FileManager.syncAdvancedSearchDependentFields();
            }
        });

        searchFilterPanel?.addEventListener('click', (e) => {
            e.stopPropagation();
        });

        document.getElementById('adv-search-close')?.addEventListener('click', (e) => {
            e.stopPropagation();
            searchFilterPanel?.classList.add('hidden');
        });

        document.getElementById('adv-search-reset')?.addEventListener('click', (e) => {
            e.preventDefault();
            e.stopPropagation();
            FileManager.resetAdvancedSearchForm();
        });

        document.getElementById('adv-search-learn')?.addEventListener('click', (e) => {
            e.preventDefault();
            e.stopPropagation();
            FileManager.showAdvancedSearchHelp();
        });

        document.getElementById('filter-owner')?.addEventListener('change', () => {
            FileManager.syncAdvancedSearchDependentFields();
        });

        document.getElementById('filter-modified')?.addEventListener('change', () => {
            FileManager.syncAdvancedSearchDependentFields();
        });

        document.getElementById('search-filter-apply')?.addEventListener('click', () => {
            FileManager.applyAdvancedSearch();
            searchFilterPanel?.classList.add('hidden');
        });

        document.getElementById('help-btn')?.addEventListener('click', (e) => {
            e.stopPropagation();
            newDropdown?.classList.add('hidden');
            searchFilterPanel?.classList.add('hidden');
            helpDropdown?.classList.toggle('hidden');
        });

        helpDropdown?.addEventListener('click', (e) => {
            e.stopPropagation();
        });

        document.getElementById('help-shortcuts')?.addEventListener('click', () => FileManager.showShortcuts());
        document.getElementById('help-feedback')?.addEventListener('click', () => {
            Components.showModal('Send feedback', '<p class="fd-muted" style="margin:0;font-size:14px;">Email your administrator or open an issue in the FreeDrive repository with steps to reproduce and screenshots.</p>', [{ text: 'OK', class: 'btn-primary' }]);
        });
        document.getElementById('help-center')?.addEventListener('click', () => {
            window.open('https://github.com/abdullaabdullazade/freedrive', '_blank', 'noopener');
        });

        document.getElementById('shortcuts-close')?.addEventListener('click', () => {
            document.getElementById('shortcuts-modal-overlay')?.classList.add('hidden');
        });

        document.getElementById('notifications-btn')?.addEventListener('click', (e) => {
            e.stopPropagation();
            FileManager.toggleNotificationsPanel();
        });

        document.getElementById('notifications-mark-read')?.addEventListener('click', () => FileManager.markAllNotificationsRead());

        // ── Profile dropdown toggle ──
        const profileBtn = document.getElementById('profile-avatar-btn');
        const profileDropdown = document.getElementById('profile-dropdown');

        profileBtn?.addEventListener('click', (e) => {
            e.stopPropagation();
            closeTransientPanels();
            const opening = profileDropdown?.classList.contains('hidden');
            profileDropdown?.classList.toggle('hidden');
            if (opening) {
                setAccountsExpanded(false);
                populateProfileDropdown();
            }
        });

        document.getElementById('profile-dropdown-close')?.addEventListener('click', (e) => {
            e.stopPropagation();
            profileDropdown?.classList.add('hidden');
        });

        document.getElementById('profile-manage-storage')?.addEventListener('click', (e) => {
            e.stopPropagation();
            profileDropdown?.classList.add('hidden');
            window.location.hash = '#/storage';
        });

        document.addEventListener('click', (e) => {
            if (!e.target.closest('#profile-dropdown, #profile-avatar-btn')) {
                profileDropdown?.classList.add('hidden');
            }
        });

        // ── Sign out ──
        document.getElementById('profile-accounts-toggle')?.addEventListener('click', (e) => {
            e.stopPropagation();
            const expanded = document.getElementById('profile-account-card')?.classList.contains('is-expanded');
            setAccountsExpanded(!expanded);
        });

        document.getElementById('logout-btn')?.addEventListener('click', async () => {
            profileDropdown?.classList.add('hidden');
            const id = API.getUser()?.id;
            const result = await API.signOutAccount(id);
            if (result?.remaining) API.reloadActive('#/files');
            else API.reloadActive('#/login');
        });

        document.getElementById('signout-all-btn')?.addEventListener('click', async () => {
            profileDropdown?.classList.add('hidden');
            await API.signOutAll();
            API.reloadActive('#/login');
        });

        document.getElementById('add-account-btn')?.addEventListener('click', (e) => {
            e.stopPropagation();
            beginAddAccount();
        });

        document.getElementById('profile-accounts')?.addEventListener('click', (e) => {
            const row = e.target.closest('[data-account-id]');
            if (!row) return;
            e.stopPropagation();
            const id = row.dataset.accountId;
            const current = API.getUser()?.id;
            profileDropdown?.classList.add('hidden');
            if (!id || id === current) return;
            switchToAccount(id);
        });

        document.querySelectorAll('.admin-nav-item[href^="/admin"]').forEach(link => {
            link.addEventListener('click', (e) => {
                e.preventDefault();
                history.pushState(null, '', link.getAttribute('href'));
                handleRoute();
            });
        });

        document.getElementById('selection-clear')?.addEventListener('click', () => FileManager.clearSelection());
        document.getElementById('bulk-share')?.addEventListener('click', () => FileManager.bulkShare());
        document.getElementById('bulk-download')?.addEventListener('click', () => FileManager.bulkDownload());
        document.getElementById('bulk-move')?.addEventListener('click', () => FileManager.bulkMove());
        document.getElementById('bulk-restore')?.addEventListener('click', () => FileManager.bulkRestore());
        document.getElementById('bulk-delete')?.addEventListener('click', () => FileManager.bulkDelete());

        document.getElementById('free-space-btn')?.addEventListener('click', () => FileManager.showLargestFiles());

        document.getElementById('details-close')?.addEventListener('click', () => FileManager.hideDetailsPanel());
        document.getElementById('details-share-btn2')?.addEventListener('click', () => FileManager.shareSelectedItem());

        document.getElementById('share-modal-close')?.addEventListener('click', () => FileManager.closeShareModal());
        document.getElementById('share-done')?.addEventListener('click', async () => {
            try {
                await FileManager.saveShareModal();
            } catch (err) {
                Components.toast(err?.message || 'Failed to save sharing settings', 'error');
            }
        });
        document.getElementById('share-copy-link')?.addEventListener('click', async () => {
            try { await FileManager.copyCurrentShareLink(); } catch { Components.toast('Failed to copy link', 'error'); }
        });

        [
            'new-folder-action',
            'file-upload-action',
            'folder-upload-action',
            'new-doc-action',
            'new-sheet-action',
            'new-presentation-action',
        ].forEach((id) => {
            document.getElementById(id)?.addEventListener('click', () => {
                newDropdown?.classList.add('hidden');
            });
        });

        document.addEventListener('keydown', (e) => FileManager.handleShortcut(e));
    }

    function showAuth() {
        const app = document.getElementById('app');
        app?.classList.remove('admin-mode');
        document.getElementById('settings-menu-admin')?.classList.add('hidden');
        document.getElementById('settings-menu-admin')?.setAttribute('aria-hidden', 'true');
        document.getElementById('auth-screen').classList.remove('hidden');
        app?.classList.add('hidden');
        Auth.syncAddingAccountUi?.();
    }

    async function showApp() {
        const user = API.getUser();
        if (user?.must_change_password) {
            Auth.showForcePasswordForm();
            return;
        }
        document.getElementById('auth-screen').classList.add('hidden');
        document.getElementById('app').classList.remove('hidden');

        if (user) {
            // Prefer server profile over stale localStorage avatar/name cache.
            if (user.avatar_url) syncAvatarCache(user.avatar_url);
            refreshUserUI();
            await refreshProfileFromServer();
        } else {
            syncAdminBtnVisibility();
        }

        const prefs = getUserPrefs();
        applyUserPrefs(prefs);
        if (!window.location.hash && prefs.startPage) {
            window.location.hash = prefs.startPage;
        }
        bindThemeMediaListener();

        SidebarTree.init();
        if (window.CryptoSync?.ensureUnlockedOnAppLoad) {
            await CryptoSync.ensureUnlockedOnAppLoad();
        }
        handleRoute();
    }

    function setLayoutMode(isAdminMode) {
        const app = document.getElementById('app');
        app?.classList.toggle('admin-mode', Boolean(isAdminMode));
        if (isAdminMode) {
            document.getElementById('details-panel')?.classList.add('hidden');
            document.getElementById('notifications-panel')?.classList.add('hidden');
            app?.classList.remove('details-open');
        }
        syncAdminBtnVisibility();
    }

    function setActiveNav(page) {
        document.querySelectorAll('.nav-item').forEach((n) => n.classList.remove('active'));
        document.getElementById(`nav-${page}`)?.classList.add('active');
    }

    async function handleRoute() {
        if (!API.isLoggedIn()) return;

        try {
        let pathRoute = window.location.pathname;
        let hash = window.location.hash;

        // If they navigate to /admin fallback or root
        if (pathRoute === '/admin') {
            history.replaceState(null, '', '/admin/dashboard');
            pathRoute = '/admin/dashboard';
        }

        const user = API.getUser() || {};
        const isAdminRoute = pathRoute.startsWith('/admin') || hash === '#/admin' || hash.startsWith('#/admin/');

        if (isAdminRoute) {
            if (String(user.role || '').toLowerCase() !== 'admin') {
                Components.toast('Admin access required', 'error');
                setLayoutMode(false);
                window.location.hash = '#/files';
                return;
            }

            setLayoutMode(true);
            let section = 'dashboard';
            
            if (hash.startsWith('#/admin/')) {
                section = (hash.split('/')[2] || 'dashboard').toLowerCase();
                // Sync path with the hash navigation
                history.replaceState(null, '', `/admin/${section}`);
                // Clear the hash so it doesn't stay in the URL
                if (window.location.hash) {
                    history.replaceState(null, '', `/admin/${section}`);
                }
            } else if (pathRoute.startsWith('/admin/')) {
                section = (pathRoute.split('/')[2] || 'dashboard').toLowerCase();
            }

            setActiveNav(`admin-${section}`);
            AdminPanel.load(section);
            return;
        }

        hash = hash || '#/files';
        setLayoutMode(false);

        if (hash === '#/home') {
            setActiveNav('home');
            FileManager.loadHome();
            return;
        }
        if (hash === '#/recent') {
            setActiveNav('recent');
            FileManager.loadRecent();
            return;
        }
        if (hash === '#/computers' || hash.startsWith('#/computers/')) {
            setActiveNav('computers');
            const folderId = hash.startsWith('#/computers/') ? hash.split('/')[2] : null;
            FileManager.loadComputerFolder(folderId);
            return;
        }
        if (hash === '#/starred') {
            setActiveNav('starred');
            FileManager.loadStarred();
            return;
        }
        if (hash === '#/shared-with') {
            setActiveNav('shared-with');
            FileManager.loadSharedWithMe();
            return;
        }
        if (hash === '#/shared-by') {
            window.location.replace('#/files');
            return;
        }
        if (hash === '#/offline') {
            setActiveNav('offline');
            FileManager.loadOffline();
            return;
        }
        if (hash === '#/trash') {
            setActiveNav('trash');
            FileManager.loadTrash();
            return;
        }
        if (hash === '#/activity') {
            setActiveNav('activity');
            FileManager.loadActivity();
            return;
        }
        if (hash === '#/storage') {
            setActiveNav('storage');
            FileManager.loadStoragePage();
            return;
        }
        if (hash.startsWith('#/open/')) {
            const openPart = hash.slice('#/open/'.length);
            const [fileId, rawQuery = ''] = openPart.split('?');
            const query = new URLSearchParams(rawQuery);
            const sharedKey = query.get('k') || '';
            setActiveNav('files');
            FileManager.loadFolder(null);
            SidebarTree.syncWithRoute();
            if (fileId) {
                setTimeout(async () => {
                    try {
                        if (sharedKey) {
                            try {
                                const keyObj = await CryptoModule.importKey(sharedKey);
                                await CryptoModule.storeKey(fileId, keyObj);
                            } catch {
                                // Continue; open flow will report if decryption fails.
                            }
                        }
                        const file = await API.files.get(fileId);
                        if (file) FileManager.openFileById(file);
                    } catch {
                        Components.toast('File not found or access denied', 'error');
                    }
                }, 400);
            }
            return;
        }

        if (hash.startsWith('#/files')) {
            setActiveNav('files');
            const folderId = hash.split('/')[2] || null;
            FileManager.loadFolder(folderId);
            SidebarTree.syncWithRoute();
            return;
        }

        window.location.hash = '#/files';
        } finally {
            syncAdminBtnVisibility();
        }
    }

    function initRipple() {
        const RIPPLE_SEL = '.btn, .btn-icon, .nav-item, .upload-btn, .context-item, .md3-chip-btn, .link-btn, .tab-btn, .sort-col';

        function attachRipple(el) {
            if (el._rippleBound) return;
            // Submenu hosts must stay overflow:visible or flyouts (e.g. Folder colour) get clipped.
            if (el.classList?.contains('has-submenu')) return;
            el._rippleBound = true;
            el.addEventListener('pointerdown', (e) => {
                if (e.target.closest('.context-submenu, .folder-color-swatch')) return;
                const rect = el.getBoundingClientRect();
                const x = e.clientX - rect.left;
                const y = e.clientY - rect.top;
                const size = Math.max(rect.width, rect.height) * 2;

                const wave = document.createElement('span');
                wave.className = 'ripple-wave';
                wave.style.cssText = `
                    width:${size}px; height:${size}px;
                    left:${x - size/2}px; top:${y - size/2}px;
                `;
                el.style.position = el.style.position || 'relative';
                el.style.overflow = 'hidden';
                el.appendChild(wave);

                wave.addEventListener('animationend', () => wave.remove(), { once: true });
            });
        }

        document.querySelectorAll(RIPPLE_SEL).forEach(attachRipple);

        // also attach to dynamically added elements via delegation
        document.addEventListener('pointerdown', (e) => {
            if (e.target.closest('.context-submenu, .folder-color-swatch, .folder-color-section')) return;
            const el = e.target.closest(RIPPLE_SEL);
            if (el && !el.classList?.contains('has-submenu')) attachRipple(el);
        }, true);
    }

    return {
        init,
        showAuth,
        showApp,
        handleRoute,
        openDriveSettings,
        openSettingsPage,
        closeSettingsPage,
    };
})();

document.addEventListener('DOMContentLoaded', () => { App.init(); });
