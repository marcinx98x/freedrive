// ========================================
// FreeDrive — Auth UI
// ========================================

const Auth = (() => {
    function friendlyAuthError(err) {
        const raw = String(err?.message || err || '').trim();
        const lower = raw.toLowerCase();
        if (!raw || lower === 'failed to fetch' || lower.includes('networkerror') || lower.includes('load failed')) {
            return 'Cannot reach the server. Check the FreeDrive URL (HTTPS / reverse proxy) and try again.';
        }
        if (lower.includes('session expired')) {
            return 'Session expired. Please sign in again.';
        }
        if (lower.includes('invalid email or password')) {
            return 'Invalid email or password. Use the exact email from account registration (Admin → Users if unsure).';
        }
        if (lower.includes('must match the invite email')) {
            return 'This invite requires the email address it was sent to. Check the invite email or ask your admin for a new invite.';
        }
        return raw || 'Something went wrong. Please try again.';
    }

    function setFormError(id, message) {
        const el = document.getElementById(id);
        if (!el) return;
        el.textContent = message || '';
    }

    function clearFormError(id) {
        setFormError(id, '');
    }

    function showForgotPasswordHelp() {
        Components.showModal(
            'Forgot password',
            `<p style="margin:0 0 12px;color:#5f6368;font-size:14px;line-height:1.45;">
                Ask your FreeDrive admin to click <strong>Reset password</strong> for your user in the admin panel.
                That sends a reset link to the email on your account (requires SMTP).
            </p>
            <p style="margin:0;color:#5f6368;font-size:14px;line-height:1.45;">
                Sign in with the <strong>exact email</strong> saved on your account — it may differ from the address in the invite email.
            </p>`,
            [{ text: 'OK', class: 'btn-primary' }]
        );
    }

    function init() {
        // Tab switching
        document.querySelectorAll('.auth-tab').forEach(tab => {
            tab.addEventListener('click', () => {
                document.querySelectorAll('.auth-tab').forEach(t => t.classList.remove('active'));
                tab.classList.add('active');

                const tabName = tab.dataset.tab;
                document.getElementById('login-form').classList.toggle('hidden', tabName !== 'login');
                document.getElementById('register-form').classList.toggle('hidden', tabName !== 'register');
                clearFormError('login-error');
                clearFormError('register-error');
            });
        });

        document.getElementById('forgot-password-btn')?.addEventListener('click', (e) => {
            e.preventDefault();
            showForgotPasswordHelp();
        });

        // Parse ?invite= / ?email= from URL
        const queryParams = new URLSearchParams(window.location.search);
        const inviteCodeParam = queryParams.get('invite');
        const inviteEmailParam = String(queryParams.get('email') || '').trim().toLowerCase();
        if (inviteCodeParam) {
            document.querySelector('.auth-tab[data-tab="register"]')?.click();
            const regInput = document.getElementById('reg-invite');
            if (regInput) regInput.value = inviteCodeParam;
            const emailInput = document.getElementById('reg-email');
            if (emailInput && inviteEmailParam) {
                emailInput.value = inviteEmailParam;
                emailInput.readOnly = true;
                emailInput.title = 'This invite requires this email address';
            }
        }

        // Parse /reset-password link mode
        const inResetMode = window.location.pathname.startsWith('/reset-password');
        if (inResetMode) {
            const token = queryParams.get('token') || '';
            const email = queryParams.get('email') || '';
            document.getElementById('login-form')?.classList.add('hidden');
            document.getElementById('register-form')?.classList.add('hidden');
            document.getElementById('reset-form')?.classList.remove('hidden');
            document.querySelector('.auth-tabs')?.classList.add('hidden');
            const titleEl = document.querySelector('.auth-logo h1');
            const subtitleEl = document.querySelector('.auth-logo .tagline');
            if (titleEl) titleEl.textContent = 'Reset password';
            if (subtitleEl) subtitleEl.textContent = 'set a new password for your account';
            const emailInput = document.getElementById('reset-email');
            if (emailInput) emailInput.value = email;

            document.getElementById('reset-form')?.addEventListener('submit', async (e) => {
                e.preventDefault();
                const newPassword = String(document.getElementById('reset-password')?.value || '');
                const confirm = String(document.getElementById('reset-password-confirm')?.value || '');
                const finalEmail = String(document.getElementById('reset-email')?.value || '').trim().toLowerCase();
                const btn = document.getElementById('reset-btn');
                clearFormError('reset-error');

                if (!token || !finalEmail) {
                    const msg = 'Invalid reset link';
                    setFormError('reset-error', msg);
                    Components.toast(msg, 'error');
                    return;
                }
                if (newPassword.length < 6) {
                    const msg = 'Password must be at least 6 characters';
                    setFormError('reset-error', msg);
                    Components.toast(msg, 'error');
                    return;
                }
                if (newPassword !== confirm) {
                    const msg = 'Passwords do not match';
                    setFormError('reset-error', msg);
                    Components.toast(msg, 'error');
                    return;
                }

                try {
                    btn.disabled = true;
                    btn.querySelector('.btn-loader').classList.remove('hidden');
                    btn.querySelector('span').textContent = 'Resetting...';
                    await API.auth.resetPassword(token, finalEmail, newPassword);
                    Components.toast('Password reset successful. Please sign in.', 'success');
                    history.replaceState(null, '', '/');
                    window.location.hash = '#/login';
                    window.location.reload();
                } catch (err) {
                    const msg = friendlyAuthError(err);
                    setFormError('reset-error', msg);
                    Components.toast(msg, 'error');
                } finally {
                    btn.disabled = false;
                    btn.querySelector('.btn-loader').classList.add('hidden');
                    btn.querySelector('span').textContent = 'Reset password';
                }
            });
        }

        // Login form
        document.getElementById('login-form').addEventListener('submit', async (e) => {
            e.preventDefault();
            const email = String(document.getElementById('login-email').value || '').trim().toLowerCase();
            const password = document.getElementById('login-password').value;
            const btn = document.getElementById('login-btn');
            clearFormError('login-error');

            try {
                btn.disabled = true;
                btn.querySelector('.btn-loader').classList.remove('hidden');
                btn.querySelector('span').textContent = 'Signing in...';

                const data = await API.auth.login(email, password);
                if (!data?.tokens?.access_token || !data?.user) {
                    throw new Error('Login response was incomplete. Please try again.');
                }
                API.setTokens(data.tokens);
                API.setUser(data.user);
                if (data.user?.avatar_url) {
                    try {
                        const prefs = JSON.parse(localStorage.getItem('fd_user_prefs') || '{}') || {};
                        prefs.profileAvatar = data.user.avatar_url;
                        localStorage.setItem('fd_user_prefs', JSON.stringify(prefs));
                        localStorage.setItem('fd_profile_photo', data.user.avatar_url);
                    } catch { /* ignore */ }
                }

                Components.toast('Welcome back, ' + data.user.username + '!', 'success');
                App.showApp();
            } catch (err) {
                const msg = friendlyAuthError(err);
                setFormError('login-error', msg);
                Components.toast(msg, 'error');
            } finally {
                btn.disabled = false;
                btn.querySelector('.btn-loader').classList.add('hidden');
                btn.querySelector('span').textContent = 'Sign In';
            }
        });

        // Register form
        document.getElementById('register-form').addEventListener('submit', async (e) => {
            e.preventDefault();
            const username = document.getElementById('reg-username').value;
            const email = String(document.getElementById('reg-email').value || '').trim().toLowerCase();
            const password = document.getElementById('reg-password').value;
            const inviteCode = String(document.getElementById('reg-invite').value || '').trim();
            const btn = document.getElementById('register-btn');
            clearFormError('register-error');

            try {
                btn.disabled = true;
                btn.querySelector('.btn-loader').classList.remove('hidden');
                btn.querySelector('span').textContent = 'Creating account...';

                await API.auth.register(email, username, password, inviteCode);

                // Auto-login after registration
                const data = await API.auth.login(email, password);
                if (!data?.tokens?.access_token || !data?.user) {
                    throw new Error('Account created, but automatic sign-in failed. Please sign in with ' + email);
                }
                API.setTokens(data.tokens);
                API.setUser(data.user);

                Components.toast('Account created. Sign-in email: ' + email, 'success', { duration: 7000 });
                App.showApp();
            } catch (err) {
                const msg = friendlyAuthError(err);
                setFormError('register-error', msg);
                Components.toast(msg, 'error');
            } finally {
                btn.disabled = false;
                btn.querySelector('.btn-loader').classList.add('hidden');
                btn.querySelector('span').textContent = 'Create Account';
            }
        });
    }

    return { init };
})();
