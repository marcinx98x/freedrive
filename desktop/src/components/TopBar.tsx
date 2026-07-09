import { useRef } from "react";
import type { SyncStatus, User } from "../types";
import { UserAvatar } from "./UserAvatar";

interface TopBarProps {
  user: User | null;
  syncStatus: SyncStatus;
  search: string;
  onSearchChange: (v: string) => void;
  onPauseResume: () => void;
  onOpenSettings: () => void;
  onProfileClick: (rect: DOMRect) => void;
}

export function TopBar({
  user,
  syncStatus,
  search,
  onSearchChange,
  onPauseResume,
  onOpenSettings,
  onProfileClick,
}: TopBarProps) {
  const avatarRef = useRef<HTMLButtonElement>(null);

  return (
    <header className="topbar">
      <div className="search-bar">
        <span>🔍</span>
        <input
          type="search"
          placeholder="Search in Drive"
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
        />
      </div>
      <div className="topbar-actions">
        <button
          type="button"
          className="icon-btn"
          title={syncStatus.paused ? "Resume sync" : "Pause sync"}
          onClick={onPauseResume}
        >
          {syncStatus.paused ? "▶" : "⏸"}
        </button>
        <button type="button" className="icon-btn" title="Settings" onClick={onOpenSettings}>
          ⚙
        </button>
        <button type="button" className="icon-btn" title="Help">
          ✦
        </button>
        <button
          ref={avatarRef}
          type="button"
          className="avatar-btn"
          title={user?.email}
          onClick={() => {
            const rect = avatarRef.current?.getBoundingClientRect();
            if (rect) onProfileClick(rect);
          }}
        >
          <UserAvatar user={user} size="sm" />
        </button>
      </div>
    </header>
  );
}
