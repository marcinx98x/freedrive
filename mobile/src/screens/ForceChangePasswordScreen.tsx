import React, { useState } from "react";
import {
  ActivityIndicator,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { ApiError, api } from "../api/client";
import { useAuth } from "../auth/AuthContext";
import { Logo } from "../components/Logo";
import { colors, radii, spacing } from "../theme";

export function ForceChangePasswordScreen() {
  const { refreshProfile, logout } = useAuth();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const onSubmit = async () => {
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
      await refreshProfile();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <SafeAreaView style={styles.safe}>
      <KeyboardAvoidingView
        style={styles.flex}
        behavior={Platform.OS === "ios" ? "padding" : undefined}
      >
        <View style={styles.container}>
          <View style={styles.logo}>
            <Logo size={56} />
          </View>
          <Text style={styles.title}>Change password</Text>
          <Text style={styles.subtitle}>
            Your administrator requires you to set a new password before continuing.
          </Text>

          <Text style={styles.label}>Current password</Text>
          <TextInput
            style={styles.input}
            value={current}
            onChangeText={setCurrent}
            secureTextEntry
            autoCapitalize="none"
            placeholder="Current password"
            placeholderTextColor={colors.textSecondary}
          />

          <Text style={styles.label}>New password</Text>
          <TextInput
            style={styles.input}
            value={next}
            onChangeText={setNext}
            secureTextEntry
            autoCapitalize="none"
            placeholder="New password (min 6 chars)"
            placeholderTextColor={colors.textSecondary}
          />

          <Text style={styles.label}>Confirm new password</Text>
          <TextInput
            style={styles.input}
            value={confirm}
            onChangeText={setConfirm}
            secureTextEntry
            autoCapitalize="none"
            placeholder="Confirm new password"
            placeholderTextColor={colors.textSecondary}
          />

          {error ? <Text style={styles.error}>{error}</Text> : null}

          <Pressable
            style={[styles.btn, loading && styles.btnDisabled]}
            onPress={() => void onSubmit()}
            disabled={loading}
          >
            {loading ? (
              <ActivityIndicator color="#fff" />
            ) : (
              <Text style={styles.btnText}>Set new password</Text>
            )}
          </Pressable>

          <Pressable onPress={() => void logout()} style={styles.linkWrap}>
            <Text style={styles.link}>Sign out</Text>
          </Pressable>
        </View>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safe: { flex: 1, backgroundColor: colors.bg },
  flex: { flex: 1 },
  container: { flex: 1, padding: spacing.lg, justifyContent: "center" },
  logo: { alignItems: "center", marginBottom: spacing.md },
  title: {
    fontSize: 22,
    fontWeight: "600",
    color: colors.text,
    textAlign: "center",
    marginBottom: spacing.sm,
  },
  subtitle: {
    fontSize: 14,
    color: colors.textSecondary,
    textAlign: "center",
    marginBottom: spacing.lg,
    lineHeight: 20,
  },
  label: { fontSize: 13, color: colors.textSecondary, marginBottom: 6, marginTop: spacing.sm },
  input: {
    height: 48,
    borderRadius: radii.md,
    borderWidth: 1,
    borderColor: colors.border,
    paddingHorizontal: 14,
    color: colors.text,
    backgroundColor: colors.surface,
  },
  error: { color: colors.danger, marginTop: spacing.md, textAlign: "center" },
  btn: {
    marginTop: spacing.lg,
    height: 48,
    borderRadius: radii.md,
    backgroundColor: colors.accent,
    alignItems: "center",
    justifyContent: "center",
  },
  btnDisabled: { opacity: 0.7 },
  btnText: { color: "#fff", fontWeight: "600", fontSize: 16 },
  linkWrap: { marginTop: spacing.md, alignItems: "center" },
  link: { color: colors.accent, fontSize: 14 },
});
