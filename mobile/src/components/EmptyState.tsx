import React, { useMemo } from "react";
import { StyleSheet, Text, View } from "react-native";
import { spacing, type ThemeColors } from "../theme";
import { useTheme } from "../theme/ThemeContext";
import { Logo } from "./Logo";

interface EmptyStateProps {
  title: string;
  subtitle?: string;
}

export function EmptyState({ title, subtitle }: EmptyStateProps) {
  const { colors } = useTheme();
  const styles = useMemo(() => makeStyles(colors), [colors]);

  return (
    <View style={styles.wrap}>
      <View style={styles.circle}>
        <Logo size={44} />
      </View>
      <Text style={styles.title}>{title}</Text>
      {subtitle ? <Text style={styles.subtitle}>{subtitle}</Text> : null}
    </View>
  );
}

function makeStyles(colors: ThemeColors) {
  return StyleSheet.create({
    wrap: {
      flex: 1,
      alignItems: "center",
      justifyContent: "center",
      paddingHorizontal: spacing.xl,
      paddingVertical: spacing.xxl,
    },
    circle: {
      width: 96,
      height: 96,
      borderRadius: 48,
      backgroundColor: colors.surfaceElevated,
      alignItems: "center",
      justifyContent: "center",
      marginBottom: spacing.lg,
    },
    title: {
      color: colors.text,
      fontSize: 20,
      fontWeight: "700",
      textAlign: "center",
      marginBottom: spacing.sm,
    },
    subtitle: {
      color: colors.textSecondary,
      fontSize: 14,
      lineHeight: 20,
      textAlign: "center",
      maxWidth: 280,
    },
  });
}
