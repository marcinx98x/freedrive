// ========================================
// FreeDrive — Auth UI
// ========================================

const Auth = (() => {
    function init() {
        // Tab switching
        document.querySelectorAll('.auth-tab').forEach(tab => {
            tab.addEventListener('click', () => {
                document.querySelectorAll('.auth-tab').forEach(t => t.classList.remove('active'));
                tab.classList.add('active');

                const tabName = tab.dataset.tab;
                document.getElementById('login-form').classList.toggle('hidden', tabName !== 'login');
                document.getElementById('register-form').classList.toggle('hidden', tabName !== 'register');
            });
        });

        // Parse ?invite= code from URL
        const queryParams = new URLSearchParams(window.location.search);
        const inviteCodeParam = queryParams.get('invite');
        if (inviteCodeParam) {
            document.querySelector('.auth-tab[data-tab="register"]')?.click();
            const regInput = document.getElementById('reg-invite');
            if (regInput) regInput.value = inviteCodeParam;
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
                const finalEmail = String(document.getElementById('reset-email')?.value || '').trim();
                const btn = document.getElementById('reset-btn');

                if (!token || !finalEmail) {
                    Components.toast('Invalid reset link', 'error');
                    return;
                }
                if (newPassword.length < 6) {
                    Components.toast('Password must be at least 6 characters', 'error');
                    return;
                }
                if (newPassword !== confirm) {
                    Components.toast('Passwords do not match', 'error');
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
                    Components.toast(err.message, 'error');
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
            const email = document.getElementById('login-email').value;
            const password = document.getElementById('login-password').value;
            const btn = document.getElementById('login-btn');

            try {
                btn.disabled = true;
                btn.querySelector('.btn-loader').classList.remove('hidden');
                btn.querySelector('span').textContent = 'Signing in...';

                const data = await API.auth.login(email, password);
                API.setTokens(data.tokens);
                API.setUser(data.user);

                Components.toast('Welcome back, ' + data.user.username + '!', 'success');
                App.showApp();
            } catch (err) {
                Components.toast(err.message, 'error');
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
            const email = document.getElementById('reg-email').value;
            const password = document.getElementById('reg-password').value;
            const inviteCode = document.getElementById('reg-invite').value;
            const btn = document.getElementById('register-btn');

            try {
                btn.disabled = true;
                btn.querySelector('.btn-loader').classList.remove('hidden');
                btn.querySelector('span').textContent = 'Creating account...';

                await API.auth.register(email, username, password, inviteCode);

                // Auto-login after registration
                const data = await API.auth.login(email, password);
                API.setTokens(data.tokens);
                API.setUser(data.user);

                Components.toast('Account created! Welcome ' + data.user.username, 'success');
                App.showApp();
            } catch (err) {
                Components.toast(err.message, 'error');
            } finally {
                btn.disabled = false;
                btn.querySelector('.btn-loader').classList.add('hidden');
                btn.querySelector('span').textContent = 'Create Account';
            }
        });
    }

    return { init };
})();
