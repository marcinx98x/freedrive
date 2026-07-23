import React from "react";
import { View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { createBottomTabNavigator } from "@react-navigation/bottom-tabs";
import { Icon, type IconName } from "../components/Icon";
import { FilesStack } from "./FilesStack";
import { HomeScreen } from "../screens/HomeScreen";
import { SharedScreen } from "../screens/SharedScreen";
import { StarredScreen } from "../screens/StarredScreen";
import { colors } from "../theme";
import type { MainTabParamList } from "./types";

const Tab = createBottomTabNavigator<MainTabParamList>();

const tabIcons: Record<keyof MainTabParamList, IconName> = {
  Home: "home",
  Starred: "star",
  Shared: "people",
  Files: "folder",
};

function TabIcon({ route, focused }: { route: keyof MainTabParamList; focused: boolean }) {
  return (
    <View
      style={{
        width: 56,
        height: 30,
        borderRadius: 15,
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: focused ? colors.fab : "transparent",
      }}
    >
      <Icon
        name={tabIcons[route]}
        size={22}
        color={focused ? colors.text : colors.textSecondary}
      />
    </View>
  );
}

export function MainTabs() {
  const insets = useSafeAreaInsets();
  return (
    <Tab.Navigator
      screenOptions={({ route }) => ({
        headerShown: false,
        tabBarStyle: {
          backgroundColor: colors.bg,
          borderTopColor: colors.border,
          height: 64 + insets.bottom,
          paddingBottom: insets.bottom + 8,
          paddingTop: 6,
        },
        tabBarActiveTintColor: colors.text,
        tabBarInactiveTintColor: colors.textSecondary,
        tabBarIcon: ({ focused }) => <TabIcon route={route.name} focused={focused} />,
        tabBarLabelStyle: { fontSize: 11, fontWeight: "600" },
      })}
    >
      <Tab.Screen name="Home" component={HomeScreen} />
      <Tab.Screen name="Starred" component={StarredScreen} />
      <Tab.Screen name="Shared" component={SharedScreen} />
      <Tab.Screen name="Files" component={FilesStack} options={{ headerShown: false }} />
    </Tab.Navigator>
  );
}
