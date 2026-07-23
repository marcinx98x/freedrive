export type FilesStackParamList = {
  FilesHome: undefined;
  Folder: { folderId: string; title?: string };
};

export type RootStackParamList = {
  Login: undefined;
  TwoFactor: { challengeId: string; emailMasked: string };
  Main: undefined;
  Search: { query: string };
  Recent: undefined;
  Trash: undefined;
  FilePreview: {
    title: string;
    uri: string;
    mime: string;
    mode: "image" | "text" | "pdf";
    text?: string;
    fileId?: string;
    gallery?: Array<{
      id: string;
      name: string;
      mime_type: string;
      iv: string;
    }>;
    index?: number;
  };
};

export type MainTabParamList = {
  Home: undefined;
  Starred: undefined;
  Shared: undefined;
  Files:
    | {
        screen?: keyof FilesStackParamList;
        params?: FilesStackParamList[keyof FilesStackParamList];
      }
    | undefined;
};
