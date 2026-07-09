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
        const fromPrefs = String(prefs?.profileAvatar || '').trim();
        if (fromPrefs) return fromPrefs;
        return String(localStorage.getItem('fd_profile_photo') || '').trim();
    }

    function syncAvatarCache(avatarUrl) {
        const prefs = getUserPrefs();
        const next = { ...prefs, profileAvatar: avatarUrl || '' };
        setUserPrefs(next);
        if (avatarUrl) {
            localStorage.setItem('fd_profile_photo', avatarUrl);
        } else {
            localStorage.removeItem('fd_profile_photo');
        }
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
        const btn = document.getElementById('admin-btn');
        if (!app || !btn) return;

        const user = API.getUser();
        const inDriveMode = !app.classList.contains('admin-mode');
        const show = inDriveMode && isAdminUser(user);

        app.classList.toggle('admin-drive-access', show);
        btn.classList.toggle('hidden', !show);
        btn.disabled = !show;
        btn.setAttribute('aria-hidden', show ? 'false' : 'true');
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
        }
        syncAdminBtnVisibility();
    }

    function applyTheme(theme) {
        const t = theme || 'system';
        const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
        const dark = t === 'dark' || (t === 'system' && prefersDark);
        document.body.classList.toggle('dark-mode', dark);
        document.body.dataset.fdTheme = t;
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

        const pendingBanner = pendingStatus.pending
            ? `<div id="settings-email-pending" style="margin-top:12px;padding:12px 14px;border-radius:8px;background:#e8f0fe;color:#174ea6;font-size:13px;line-height:1.45;">
                Check your inbox at <strong>${esc(pendingStatus.new_email_masked || 'your new address')}</strong>.
                The confirmation link expires ${esc(formatEmailExpiry(pendingStatus.expires_at))}.
            </div>`
            : '';

        Components.showModal('Settings', `
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
                        <span style="font-size:13px;font-weight:500;color:#5f6368;display:block;margin-bottom:6px;">First name</span>
                        <input id="settings-first-name" type="text" value="${esc(firstName)}" placeholder="First name" style="width:100%;height:36px;border-radius:8px;border:1px solid #dadce0;padding:0 12px;font-size:14px;background:#fff;">
                    </label>
                    <label class="drive-settings-field drive-settings-field-full">
                        <span style="font-size:13px;font-weight:500;color:#5f6368;display:block;margin-bottom:6px;">Last name</span>
                        <input id="settings-last-name" type="text" value="${esc(lastName)}" placeholder="Last name" style="width:100%;height:36px;border-radius:8px;border:1px solid #dadce0;padding:0 12px;font-size:14px;background:#fff;">
                    </label>
                    <label class="drive-settings-field drive-settings-field-full">
                        <span style="font-size:13px;font-weight:500;color:#5f6368;display:block;margin-bottom:6px;">Email</span>
                        <input id="settings-email" type="email" value="${esc(user.email || '')}" placeholder="Email" autocomplete="email" style="width:100%;height:36px;border-radius:8px;border:1px solid #dadce0;padding:0 12px;font-size:14px;background:#fff;">
                        <span style="display:block;margin-top:6px;font-size:12px;color:#5f6368;">Confirmation link will be sent to the new address.</span>
                    </label>
                    <label class="drive-settings-field drive-settings-field-full hidden" id="settings-email-password-wrap">
                        <span style="font-size:13px;font-weight:500;color:#5f6368;display:block;margin-bottom:6px;">Current password</span>
                        <input id="settings-email-password" type="password" placeholder="Required to change email" autocomplete="current-password" style="width:100%;height:36px;border-radius:8px;border:1px solid #dadce0;padding:0 12px;font-size:14px;background:#fff;">
                    </label>
                    <button type="button" class="btn btn-secondary drive-settings-confirm-btn" id="settings-send-email-confirm">Confirm</button>
                    ${pendingBanner}
                    <div style="margin-top:20px;padding-top:20px;border-top:1px solid #e8eaed;">
                        <div style="font-size:13px;font-weight:500;color:#5f6368;margin-bottom:6px;">Encryption keys across devices</div>
                        <p style="margin:0 0 12px;font-size:12px;color:#5f6368;line-height:1.45;">
                            Encryption keys sync automatically across your devices when you sign in.
                            Use export/import below only as a backup.
                        </p>
                        <div style="display:flex;flex-wrap:wrap;gap:8px;">
                            <button type="button" class="btn btn-secondary" id="settings-export-keys-btn">Export encryption keys</button>
                            <button type="button" class="btn btn-secondary" id="settings-import-keys-btn">Import encryption keys</button>
                        </div>
                        <input type="file" id="settings-import-keys-input" accept="application/json,.json" hidden>
                    </div>
                </div>
                <input id="settings-avatar-input" type="file" accept="image/*" hidden>
            </div>
        `, [
            { text: 'Cancel' },
            {
                text: 'Save',
                class: 'btn-primary',
                close: false,
                action: async () => {
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
                        Components.hideModal();
                    } catch (err) {
                        Components.toast(err?.message || 'Failed to save profile', 'error');
                    }
                },
            },
        ]);

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

        document.getElementById('settings-export-keys-btn')?.addEventListener('click', async () => {
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

        const importKeysInput = document.getElementById('settings-import-keys-input');
        document.getElementById('settings-import-keys-btn')?.addEventListener('click', () => {
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
                Components.hideModal();
                openDriveSettings();
            } catch (err) {
                Components.toast(err?.message || 'Failed to request email change', 'error');
            } finally {
                sendConfirmBtn.disabled = false;
                sendConfirmBtn.textContent = prevLabel;
            }
        });
    }

    async function openSecurityCenter() {
        const esc = Components.escapeHtml;
        let profile = API.getUser() || {};
        try {
            profile = await API.me();
            API.setUser(profile);
        } catch { /* use cached */ }

        const required = Boolean(profile.two_factor_required);
        const enabled = Boolean(profile.email_2fa_enabled) || required;
        const toggleDisabled = required ? 'disabled' : '';
        const requiredNote = required
            ? '<p style="margin:12px 0 0;font-size:12px;color:#174ea6;line-height:1.45;">Your administrator requires email two-factor authentication for all accounts.</p>'
            : '<p style="margin:12px 0 0;font-size:12px;color:#5f6368;line-height:1.45;">When enabled, you will receive a 6-digit code by email each time you sign in.</p>';

        Components.showModal('Security', `
            <div class="drive-settings-modal" style="padding:8px 0;">
                <div style="border:1px solid #e8eaed;border-radius:12px;padding:16px 18px;background:#fff;">
                    <div style="display:flex;align-items:flex-start;justify-content:space-between;gap:16px;">
                        <div>
                            <div style="font-size:15px;font-weight:600;color:#202124;margin-bottom:4px;">Email two-factor authentication</div>
                            <div style="font-size:13px;color:#5f6368;line-height:1.45;">Protect your account with a verification code sent to ${esc(profile.email || 'your email')}.</div>
                        </div>
                        <label class="live-toggle" style="flex-shrink:0;display:inline-flex;align-items:center;gap:8px;cursor:pointer;">
                            <input type="checkbox" id="security-2fa-toggle" ${enabled ? 'checked' : ''} ${toggleDisabled}>
                        </label>
                    </div>
                    ${requiredNote}
                </div>
            </div>
        `, [{ text: 'Close' }]);

        const toggle = document.getElementById('security-2fa-toggle');
        toggle?.addEventListener('change', async () => {
            if (required) {
                toggle.checked = true;
                Components.toast('Two-factor authentication is required by your administrator', 'info');
                return;
            }
            const next = Boolean(toggle.checked);
            const prev = !next;
            try {
                const updated = await API.updateMe({ email_2fa_enabled: next });
                API.setUser(updated);
                Components.toast(next ? 'Email two-factor authentication enabled' : 'Email two-factor authentication disabled', 'success');
            } catch (err) {
                toggle.checked = prev;
                Components.toast(err?.message || 'Failed to update security setting', 'error');
            }
        });
    }

    function init() {
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
            showApp();
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

        document.getElementById('topbar-settings')?.addEventListener('click', () => {
            openDriveSettings();
        });
        document.getElementById('topbar-security')?.addEventListener('click', () => {
            openSecurityCenter();
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
            Components.showModal('Send feedback', '<p style="margin:0;color:#5f6368;font-size:14px;line-height:1.45;">Email your administrator or open an issue in the FreeDrive repository with steps to reproduce and screenshots.</p>', [{ text: 'OK', class: 'btn-primary' }]);
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
            profileDropdown?.classList.toggle('hidden');
            // Populate user info
            const user = API.getUser?.() || JSON.parse(localStorage.getItem('fd_user') || '{}');
            const displayName = String(
                user.username
                || user.name
                || (user.email ? String(user.email).split('@')[0] : '')
                || 'User'
            ).trim();
            document.getElementById('profile-name').textContent = displayName;
            if (user.email) document.getElementById('profile-email').textContent = user.email;
            
            const initial = Components.initials(displayName || user.email || 'U');
            const lgAvatar = document.getElementById('profile-avatar-lg');
            const prefs = getUserPrefs();
            const savedPhoto = prefs.profileAvatar || localStorage.getItem('fd_profile_photo');
            
            if (lgAvatar) {
                if (savedPhoto) {
                    lgAvatar.innerHTML = '';
                    lgAvatar.style.backgroundImage = `url(${savedPhoto})`;
                    lgAvatar.style.backgroundSize = 'cover';
                    lgAvatar.style.backgroundPosition = 'center';
                } else {
                    lgAvatar.style.backgroundImage = '';
                    lgAvatar.textContent = initial;
                }
            }
        });

        document.addEventListener('click', (e) => {
            if (!e.target.closest('#profile-dropdown, #profile-avatar-btn')) {
                profileDropdown?.classList.add('hidden');
            }
        });

        // ── Change profile photo ──
        document.getElementById('profile-change-photo')?.addEventListener('click', () => {
            document.getElementById('profile-photo-input')?.click();
        });

        document.getElementById('profile-photo-input')?.addEventListener('change', async (e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            const reader = new FileReader();
            reader.onload = async (ev) => {
                try {
                    const result = await resizeAvatarDataURL(String(ev.target?.result || ''));
                    const updated = await API.updateMe({ avatar_url: result });
                    API.setUser(updated);
                    syncAvatarCache(updated.avatar_url || result);
                    refreshUserUI();

                    const lgAvatar = document.getElementById('profile-avatar-lg');
                    if (lgAvatar) {
                        lgAvatar.innerHTML = '';
                        lgAvatar.style.backgroundImage = `url(${updated.avatar_url || result})`;
                        lgAvatar.style.backgroundSize = 'cover';
                        lgAvatar.style.backgroundPosition = 'center';
                    }

                    Components.toast('Profile photo updated', 'success');
                } catch (err) {
                    Components.toast(err?.message || 'Failed to update profile photo', 'error');
                }
            };
            reader.readAsDataURL(file);
            profileDropdown?.classList.add('hidden');
            e.target.value = '';
        });

        // ── Keyboard shortcuts ──
        document.getElementById('profile-keyboard-btn')?.addEventListener('click', () => {
            profileDropdown?.classList.add('hidden');
            FileManager.showShortcuts?.();
        });

        document.getElementById('profile-settings-btn')?.addEventListener('click', () => {
            profileDropdown?.classList.add('hidden');
            openDriveSettings();
        });

        document.getElementById('profile-security-btn')?.addEventListener('click', () => {
            profileDropdown?.classList.add('hidden');
            openSecurityCenter();
        });

        document.getElementById('profile-toggle-view-btn')?.addEventListener('click', () => {
            profileDropdown?.classList.add('hidden');
            const gridBtn = document.getElementById('topbar-view-grid');
            const listBtn = document.getElementById('topbar-view-list');
            if (gridBtn?.classList.contains('active')) {
                listBtn?.click();
            } else {
                gridBtn?.click();
            }
        });

        // ── Sign out ──
        document.getElementById('logout-btn')?.addEventListener('click', async () => {
            profileDropdown?.classList.add('hidden');
            try { await API.auth.logout(); } catch {}
            API.clearAuth();
            SidebarTree.invalidateAll();
            showAuth();
        });

        document.getElementById('admin-btn')?.addEventListener('click', (e) => {
            e.preventDefault();
            history.pushState(null, '', '/admin/dashboard');
            handleRoute();
        });

        document.querySelectorAll('.admin-nav-item[href^="/admin"]').forEach(link => {
            link.addEventListener('click', (e) => {
                e.preventDefault();
                history.pushState(null, '', link.getAttribute('href'));
                handleRoute();
            });
        });

        document.getElementById('bulk-share')?.addEventListener('click', () => FileManager.bulkShare());
        document.getElementById('bulk-download')?.addEventListener('click', () => FileManager.bulkDownload());
        document.getElementById('bulk-move')?.addEventListener('click', () => FileManager.bulkMove());
        document.getElementById('bulk-restore')?.addEventListener('click', () => FileManager.bulkRestore());
        document.getElementById('bulk-delete')?.addEventListener('click', () => FileManager.bulkDelete());

        document.getElementById('free-space-btn')?.addEventListener('click', () => FileManager.showLargestFiles());

        document.getElementById('details-close')?.addEventListener('click', () => FileManager.hideDetailsPanel());
        document.getElementById('details-share-btn')?.addEventListener('click', () => FileManager.shareSelectedItem());
        document.getElementById('details-share-btn2')?.addEventListener('click', () => FileManager.shareSelectedItem());
        document.getElementById('details-download-btn')?.addEventListener('click', () => FileManager.downloadSelected());
        document.getElementById('details-rename-btn')?.addEventListener('click', () => FileManager.renameSelected());
        document.getElementById('details-delete-btn')?.addEventListener('click', () => FileManager.deleteSelected());

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
        document.getElementById('share-copy-link-footer')?.addEventListener('click', async () => {
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
        app?.classList.remove('admin-mode', 'admin-drive-access');
        document.getElementById('admin-btn')?.classList.add('hidden');
        document.getElementById('auth-screen').classList.remove('hidden');
        app?.classList.add('hidden');
    }

    async function showApp() {
        document.getElementById('auth-screen').classList.add('hidden');
        document.getElementById('app').classList.remove('hidden');

        const user = API.getUser();
        if (user) {
            // Prefer server profile over stale localStorage avatar/name cache.
            if (user.avatar_url) syncAvatarCache(user.avatar_url);
            refreshUserUI();
            await refreshProfileFromServer();
        } else {
            syncAdminBtnVisibility();
        }

        const prefs = getUserPrefs();
        applyTheme(prefs.theme || 'system');
        if (!window.location.hash && prefs.startPage) {
            window.location.hash = prefs.startPage;
        }

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
            el._rippleBound = true;
            el.addEventListener('pointerdown', (e) => {
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
            const el = e.target.closest(RIPPLE_SEL);
            if (el) attachRipple(el);
        }, true);
    }

    return {
        init,
        showAuth,
        showApp,
        handleRoute,
    };
})();

document.addEventListener('DOMContentLoaded', App.init);
