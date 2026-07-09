type NavIconName = "home" | "sync" | "notifications";

const paths: Record<NavIconName, string> = {
  home: "M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z",
  sync: "M12 4V1L8 5l4 4V6c3.31 0 6 2.69 6 6 0 1.01-.25 1.97-.7 2.8l1.46 1.46C19.54 15.03 20 13.57 20 12c0-4.42-3.58-8-8-8zm0 14c-3.31 0-6-2.69-6-6 0-1.01.25-1.97.7-2.8L5.24 7.74C4.46 8.97 4 10.43 4 12c0 4.42 3.58 8 8 8v3l4-4-4-4v3z",
  notifications:
    "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z",
};

interface NavIconProps {
  name: NavIconName;
  className?: string;
}

export function NavIcon({ name, className }: NavIconProps) {
  return (
    <svg
      className={className ?? "nav-icon"}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden
    >
      <path d={paths[name]} />
    </svg>
  );
}
