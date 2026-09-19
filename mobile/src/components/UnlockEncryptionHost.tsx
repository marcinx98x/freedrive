import React, { useCallback, useEffect, useRef, useState } from "react";
import { Modal, Pressable, StyleSheet, Text, View } from "react-native";
import { useAuth } from "../auth/AuthContext";
import { isUnlocked } from "../crypto";
import {
  registerUnlockPrompt,
  type UnlockPromptReason,
} from "../crypto/ensureUnlocked";
import { radii, spacing } from "../theme";
import { useTheme } from "../theme/ThemeContext";
import { UnlockEncryptionModal } from "./UnlockEncryptionModal";

type Pending = {
  userId: string;
  reason: UnlockPromptReason;
  resolve: (ok: boolean) => void;
};

/**
 * Registers the mid-session password unlock prompt and shows a centered
 * notice when login succeeded but encryption stayed locked.
 */
export function UnlockEncryptionHost() {
  const { user, cryptoUnlocked, cryptoUnlockHint, clearCryptoUnlockHint, markCryptoUnlocked } =
    useAuth();
  const { colors } = useTheme();
  const [pending, setPending] = useState<Pending | null>(null);
  const pendingRef = useRef<Pending | null>(null);

  const settle = useCallback(
    (ok: boolean) => {
      const current = pendingRef.current;
      pendingRef.current = null;
      setPending(null);
      if (ok) {
        markCryptoUnlocked();
        clearCryptoUnlockHint();
      }
      current?.resolve(ok);
    },
    [markCryptoUnlocked, clearCryptoUnlockHint],
  );

  const openUnlockPrompt = useCallback(() => {
    if (!user) return;
    void (async () => {
      const ok = await new Promise<boolean>((resolve) => {
        pendingRef.current?.resolve(false);
        const next: Pending = {
          userId: user.id,
          reason: "generic",
          resolve,
        };
        pendingRef.current = next;
        setPending(next);
      });
      if (ok) clearCryptoUnlockHint();
    })();
  }, [user, clearCryptoUnlockHint]);

  useEffect(() => {
    registerUnlockPrompt(({ userId, reason }) => {
      if (isUnlocked()) return Promise.resolve(true);
      return new Promise<boolean>((resolve) => {
        pendingRef.current?.resolve(false);
        const next: Pending = { userId, reason, resolve };
        pendingRef.current = next;
        setPending(next);
      });
    });
    return () => registerUnlockPrompt(null);
  }, []);

  useEffect(() => {
    if (!user && pendingRef.current) {
      settle(false);
    }
  }, [user, settle]);

  const showHint = Boolean(user && cryptoUnlockHint && !cryptoUnlocked && !pending);

  return (
    <>
      <Modal visible={showHint} transparent animationType="fade" onRequestClose={clearCryptoUnlockHint}>
        <View style={[styles.backdrop, { backgroundColor: colors.overlay }]}>
          <View style={[styles.card, { backgroundColor: colors.surfaceElevated }]}>
            <Text style={[styles.title, { color: colors.text }]}>Encryption locked</Text>
            <Text style={[styles.body, { color: colors.textSecondary }]}>
              {cryptoUnlockHint}
            </Text>
            <View style={styles.actions}>
              <Pressable onPress={clearCryptoUnlockHint} style={styles.actionBtn} hitSlop={8}>
                <Text style={[styles.actionText, { color: colors.textSecondary }]}>Dismiss</Text>
              </Pressable>
              <Pressable onPress={openUnlockPrompt} style={styles.actionBtn} hitSlop={8}>
                <Text style={[styles.actionText, { color: colors.accentMuted }]}>Unlock</Text>
              </Pressable>
            </View>
          </View>
        </View>
      </Modal>

      <UnlockEncryptionModal
        visible={Boolean(pending)}
        userId={pending?.userId || user?.id || ""}
        reason={pending?.reason || "generic"}
        onSuccess={() => settle(true)}
        onCancel={() => settle(false)}
      />
    </>
  );
}

const styles = StyleSheet.create({
  backdrop: {
    flex: 1,
    justifyContent: "center",
    paddingHorizontal: spacing.xl,
  },
  card: {
    borderRadius: radii.lg,
    padding: spacing.xl,
  },
  title: {
    fontSize: 18,
    fontWeight: "600",
    marginBottom: spacing.sm,
  },
  body: {
    fontSize: 14,
    lineHeight: 20,
    marginBottom: spacing.xl,
  },
  actions: {
    flexDirection: "row",
    justifyContent: "flex-end",
    gap: spacing.lg,
  },
  actionBtn: {
    minWidth: 64,
    alignItems: "center",
    paddingVertical: spacing.sm,
  },
  actionText: {
    fontWeight: "600",
    fontSize: 15,
  },
});
