import { useState } from "react";
import { api } from "../api/tauri";
import { Logo } from "../components/Logo";

interface ForceChangePasswordProps {
  onSuccess: () => void;
  onLogout: () => void;
}

export function ForceChangePassword({ onSuccess, onLogout }: ForceChangePasswordProps) {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    if (!current || !next) {
      setError("Enter your current and new password");
      return;
    }
    if (next.length < 6) {
      setError("Password must be at least 6 characters");
      return;
    }
    if (next !== confirm) {
      setError("Passwords do not match");
      return;
    }
    setLoading(true);
    try {
      await api.changePassword(current, next);
      onSuccess();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="signin-layout">
      <div className="signin-left">
        <div className="signin-header">
          <Logo size={36} />
        </div>
        <h1 className="signin-title">Change password</h1>
        <p className="signin-subtitle">
          Your administrator requires you to set a new password before continuing.
        </p>
        {error ? <div className="error-banner">{error}</div> : null}
        <form onSubmit={(e) => void handleSubmit(e)}>
          <div className="form-group">
            <label htmlFor="force-current">Current password</label>
            <input
              id="force-current"
              type="password"
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
              autoComplete="current-password"
              required
            />
          </div>
          <div className="form-group">
            <label htmlFor="force-new">New password</label>
            <input
              id="force-new"
              type="password"
              value={next}
              onChange={(e) => setNext(e.target.value)}
              autoComplete="new-password"
              minLength={6}
              required
            />
          </div>
          <div className="form-group">
            <label htmlFor="force-confirm">Confirm new password</label>
            <input
              id="force-confirm"
              type="password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
              autoComplete="new-password"
              minLength={6}
              required
            />
          </div>
          <button type="submit" className="btn-primary" disabled={loading}>
            {loading ? "Saving…" : "Set new password"}
          </button>
          <button type="button" className="btn-text" onClick={() => void onLogout()}>
            Sign out
          </button>
        </form>
      </div>
      <div className="signin-right">
        <div className="signin-illustration" aria-hidden />
      </div>
    </div>
  );
}
