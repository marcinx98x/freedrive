import React from "react";
import { ActivityIndicator, View } from "react-native";
import { NavigationContainer, DarkTheme } from "@react-navigation/native";
import { createNativeStackNavigator } from "@react-navigation/native-stack";
import { useAuth } from "../auth/AuthContext";
import { FilePreviewScreen } from "../screens/FilePreviewScreen";
import { LoginScreen } from "../screens/LoginScreen";
import { RecentScreen } from "../screens/RecentScreen";
import { SearchScreen } from "../screens/SearchScreen";
import { TrashScreen } from "../screens/TrashScreen";
import { TwoFactorScreen } from "../screens/TwoFactorScreen";
import { colors } from "../theme";
import { MainTabs } from "./MainTabs";
import type { RootStackParamList } from "./types";

const Stack = createNativeStackNavigator<RootStackParamList>();

const navTheme = {
  ...DarkTheme,
  colors: {
    ...DarkTheme.colors,
    background: colors.bg,
    card: colors.bg,
    text: colors.text,
    border: colors.border,
    primary: colors.accent,
  },
};

export function RootNavigator() {
  const { booting, signedIn } = useAuth();

  if (booting) {
    return (
      <View style={{ flex: 1, backgroundColor: colors.bg, alignItems: "center", justifyContent: "center" }}>
        <ActivityIndicator color={colors.accent} size="large" />
      </View>
    );
  }

  return (
    <NavigationContainer theme={navTheme}>
      <Stack.Navigator screenOptions={{ headerShown: false, contentStyle: { backgroundColor: colors.bg } }}>
        {!signedIn ? (
          <>
            <Stack.Screen name="Login" component={LoginScreen} />
            <Stack.Screen name="TwoFactor" component={TwoFactorScreen} />
          </>
        ) : (
          <>
            <Stack.Screen name="Main" component={MainTabs} />
            <Stack.Screen
              name="Search"
              component={SearchScreen}
              options={{ headerShown: true }}
            />
            <Stack.Screen
              name="Recent"
              component={RecentScreen}
              options={{ headerShown: true }}
            />
            <Stack.Screen
              name="Trash"
              component={TrashScreen}
              options={{ headerShown: true }}
            />
            <Stack.Screen
              name="FilePreview"
              component={FilePreviewScreen}
              options={{ headerShown: true }}
            />
          </>
        )}
      </Stack.Navigator>
    </NavigationContainer>
  );
}
