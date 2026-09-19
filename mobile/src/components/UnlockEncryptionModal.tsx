import React, { useEffect, useState } from "react";
import {
  ActivityIndicator,
  KeyboardAvoidingView,
  Modal,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { isUnlocked, unlockWithPassword } from "../crypto";
import type { UnlockPromptReason } from "../crypto/ensureUnlocked";
import { radii, spacing } from "../theme";
import { useTheme } from "../theme/ThemeContext";

type Props = {
  visible: boolean;
  userId: string;
  reason: UnlockPromptReason;
  onSuccess: () => void;
  onCancel: () => void;
};

function subtitleFor(reason: UnlockPromptReason): string {
  if (reason === "open") {
    return "Enter your account password to decrypt and open files on this device.";
  }
  if (reason === "upload") {
    return "Enter your account password to encrypt and upload files on this device.";
  }
  return "Enter your account password to unlock encryption on this device.";
}

export function UnlockEncryptionModal({
  visible,
  userId,
  reason,
  onSuccess,
  onCancel,
}: Props) {
  const { colors } = useTheme();
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (visible) {
      setPassword("");
      setError("");
      setBusy(false);
    }
  }, [visible]);

  const submit = async () => {
    if (!password) {
      setError("Enter your password");
      return;
    }
    setBusy(true);
    setError("");
    try {
      await unlockWithPassword(password, userId);
      if (!isUnlocked()) {
        setError("Could not unlock encryption with that password.");
        setBusy(false);
        return;
      }
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  };

  return (
    <Modal visible={visible} transparent animationType="fade" onRequestClose={onCancel}>
      <KeyboardAvoidingView
        style={styles.flex}
        behavior={Platform.OS === "ios" ? "padding" : undefined}
      >
        <View style={[styles.backdrop, { backgroundColor: colors.overlay }]}>
          <View style={[styles.card, { backgroundColor: colors.surfaceElevated }]}>
            <Text style={[styles.title, { color: colors.text }]}>Unlock encryption</Text>
            <Text style={[styles.subtitle, { color: colors.textSecondary }]}>
              {subtitleFor(reason)}
            </Text>
            <TextInput
              style={[
                styles.input,
                {
                  borderColor: colors.accent,
                  color: colors.text,
                  backgroundColor: colors.inputBg,
                },
              ]}
              value={password}
              onChangeText={setPassword}
              secureTextEntry
              autoCapitalize="none"
              autoCorrect={false}
              autoFocus
              editable={!busy}
              placeholder="Password"
              placeholderTextColor={colors.textSecondary}
              onSubmitEditing={() => void submit()}
              returnKeyType="done"
            />
            {error ? <Text style={[styles.error, { color: colors.danger }]}>{error}</Text> : null}
            <View style={styles.actions}>
              <Pressable onPress={onCancel} disabled={busy} style={styles.actionBtn}>
                <Text style={[styles.actionText, { color: colors.accentMuted }]}>Cancel</Text>
              </Pressable>
              <Pressable onPress={() => void submit()} disabled={busy} style={styles.actionBtn}>
                {busy ? (
                  <ActivityIndicator color={colors.accent} />
                ) : (
                  <Text style={[styles.actionText, { color: colors.accentMuted }]}>Unlock</Text>
                )}
              </Pressable>
            </View>
          </View>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1 },
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
  subtitle: {
    fontSize: 14,
    lineHeight: 20,
    marginBottom: spacing.lg,
  },
  input: {
    borderWidth: 2,
    borderRadius: radii.sm,
    paddingHorizontal: spacing.md,
    paddingVertical: spacing.md,
    fontSize: 16,
  },
  error: {
    marginTop: spacing.sm,
    fontSize: 13,
  },
  actions: {
    flexDirection: "row",
    justifyContent: "flex-end",
    gap: spacing.lg,
    marginTop: spacing.xl,
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
