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
            if (!user?.id) return null;
            API.setUser(user);
            syncAvatarCache(user.avatar_url || '');
            refreshUserUI();
            return user;
        } catch {
            return null;
        }
    }

    function refreshUserUI() {
        const user = API.getUser();
        if (!user) return;
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

    function applyTheme(theme) {
        const t = theme || 'system';
        const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
        const dark = t === 'dark' || (t === 'system' && prefersDark);
        document.body.classList.toggle('dark-mode', dark);
        document.body.dataset.fdTheme = t;
    }

    function openDriveSettings() {
        const user = API.getUser() || {};
        const prefs = getUserPrefs();
        const esc = Components.escapeHtml;
        const currentName = user.username || '';
        const nameParts = currentName.split(' ');
        const firstName = nameParts[0] || '';
        const lastName = nameParts.slice(1).join(' ') || '';
        const previewAvatar = resolveAvatar(user, prefs);

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
            Components.toast('Security center will be available soon', 'info');
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
        document.getElementById('help-feedback')?.addEventListener('click', () => Components.toast('Feedback form opened', 'info'));
        document.getElementById('help-center')?.addEventListener('click', () => Components.toast('Help center is available in-app', 'info'));

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

        document.getElementById('profile-photo-input')?.addEventListener('change', (e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            const reader = new FileReader();
            reader.onload = (ev) => {
                const dataUrl = ev.target.result;
                localStorage.setItem('fd_profile_photo', dataUrl);
                // Update all avatars
                document.getElementById('profile-avatar-lg').innerHTML = `<img src="${dataUrl}" alt="Profile">`;
                const topAvatar = document.getElementById('topbar-avatar');
                if (topAvatar) {
                    topAvatar.innerHTML = `<img src="${dataUrl}" alt="Profile" style="width:100%;height:100%;object-fit:cover;border-radius:50%">`;
                }
                Components.toast('Profile photo updated', 'success');
            };
            reader.readAsDataURL(file);
            profileDropdown?.classList.add('hidden');
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
            Components.toast('Security center will be available soon', 'info');
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
        document.getElementById('app')?.classList.remove('admin-mode');
        document.getElementById('auth-screen').classList.remove('hidden');
        document.getElementById('app').classList.add('hidden');
    }

    function showApp() {
        document.getElementById('auth-screen').classList.add('hidden');
        document.getElementById('app').classList.remove('hidden');

        const user = API.getUser();
        if (user) {
            // Prefer server profile over stale localStorage avatar/name cache.
            if (user.avatar_url) syncAvatarCache(user.avatar_url);
            refreshUserUI();
            if (user.role === 'admin') {
                const ab = document.getElementById('admin-btn');
                if (ab) ab.style.display = '';
            }
            refreshProfileFromServer().then((fresh) => {
                if (fresh?.role === 'admin') {
                    const ab = document.getElementById('admin-btn');
                    if (ab) ab.style.display = '';
                }
            });
        }

        const prefs = getUserPrefs();
        applyTheme(prefs.theme || 'system');
        if (!window.location.hash && prefs.startPage) {
            window.location.hash = prefs.startPage;
        }

        SidebarTree.init();
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
    }

    function setActiveNav(page) {
        document.querySelectorAll('.nav-item').forEach((n) => n.classList.remove('active'));
        document.getElementById(`nav-${page}`)?.classList.add('active');
    }

    async function handleRoute() {
        if (!API.isLoggedIn()) return;
        
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
            if (hash !== '#/computers') {
                window.location.hash = '#/computers';
            }
            FileManager.loadComputers();
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
            setActiveNav('shared-by');
            FileManager.loadSharedByMe();
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
