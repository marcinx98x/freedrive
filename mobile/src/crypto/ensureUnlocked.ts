import { isUnlocked, tryRestoreUnlock } from "./index";

export type UnlockPromptReason = "upload" | "open" | "generic";

type UnlockPromptFn = (opts: {
  userId: string;
  reason: UnlockPromptReason;
}) => Promise<boolean>;

let promptFn: UnlockPromptFn | null = null;

export function registerUnlockPrompt(fn: UnlockPromptFn | null): void {
  promptFn = fn;
}

function cancelledMessage(reason: UnlockPromptReason): string {
  if (reason === "open") {
    return "Encryption unlock cancelled. Enter your password to open encrypted files.";
  }
  if (reason === "upload") {
    return "Encryption unlock cancelled. Enter your password to upload encrypted files.";
  }
  return "Encryption unlock cancelled. Enter your password to continue.";
}

/**
 * Ensure UEK is in memory: try SecureStore restore, then password modal if registered.
 * Throws if the user cancels or unlock is still impossible.
 */
export async function ensureUnlockedOrPrompt(
  userId: string,
  reason: UnlockPromptReason = "generic",
): Promise<void> {
  if (!userId) {
    throw new Error("Sign out and sign in again with your password to unlock encryption.");
  }
  if (isUnlocked()) return;
  if (await tryRestoreUnlock(userId)) return;

  if (!promptFn) {
    throw new Error("Sign out and sign in again with your password to unlock encryption.");
  }

  const ok = await promptFn({ userId, reason });
  if (!ok || !isUnlocked()) {
    throw new Error(cancelledMessage(reason));
  }
}
