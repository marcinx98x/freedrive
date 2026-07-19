export type RootStackParamList = {
  Login: undefined;
  TwoFactor: { challengeId: string; emailMasked: string };
  Main: undefined;
  Folder: { folderId: string; title?: string };
  Search: { query: string };
  Recent: undefined;
  Trash: undefined;
};

export type MainTabParamList = {
  Home: undefined;
  Starred: undefined;
  Shared: undefined;
  Files: undefined;
};
