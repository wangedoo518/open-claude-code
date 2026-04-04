import { createSlice, type PayloadAction } from "@reduxjs/toolkit";
import {
  CODE_TOOL_IDS,
  DEFAULT_CODE_TOOL,
  type CodeToolId,
  type SelectedCodeToolModel,
} from "@/features/code-tools";

const MAX_DIRECTORIES = 10;

export interface CodeToolsState {
  selectedCliTool: CodeToolId;
  selectedModels: Record<CodeToolId, SelectedCodeToolModel | null>;
  environmentVariables: Record<CodeToolId, string>;
  directories: string[];
  currentDirectory: string;
  selectedTerminal: string;
}

function createSelectionRecord<T>(initialValue: T): Record<CodeToolId, T> {
  return CODE_TOOL_IDS.reduce(
    (acc, toolId) => {
      acc[toolId] = initialValue;
      return acc;
    },
    {} as Record<CodeToolId, T>
  );
}

export const initialState: CodeToolsState = {
  selectedCliTool: DEFAULT_CODE_TOOL,
  selectedModels: createSelectionRecord<SelectedCodeToolModel | null>(null),
  environmentVariables: createSelectionRecord(""),
  directories: [],
  currentDirectory: "",
  selectedTerminal: "Terminal",
};

const codeToolsSlice = createSlice({
  name: "codeTools",
  initialState,
  reducers: {
    setSelectedCliTool(state, action: PayloadAction<CodeToolId>) {
      state.selectedCliTool = action.payload;
    },
    setSelectedTerminal(state, action: PayloadAction<string>) {
      state.selectedTerminal = action.payload;
    },
    setSelectedModel(
      state,
      action: PayloadAction<SelectedCodeToolModel | null>
    ) {
      state.selectedModels[state.selectedCliTool] = action.payload;
    },
    setEnvironmentVariables(state, action: PayloadAction<string>) {
      state.environmentVariables[state.selectedCliTool] = action.payload;
    },
    addDirectory(state, action: PayloadAction<string>) {
      const directory = action.payload.trim();
      if (!directory) return;
      state.directories = [
        directory,
        ...state.directories.filter((entry) => entry !== directory),
      ].slice(0, MAX_DIRECTORIES);
    },
    removeDirectory(state, action: PayloadAction<string>) {
      state.directories = state.directories.filter(
        (directory) => directory !== action.payload
      );
      if (state.currentDirectory === action.payload) {
        state.currentDirectory = "";
      }
    },
    setCurrentDirectory(state, action: PayloadAction<string>) {
      const directory = action.payload.trim();
      state.currentDirectory = directory;
      if (!directory) {
        return;
      }
      state.directories = [
        directory,
        ...state.directories.filter((entry) => entry !== directory),
      ].slice(0, MAX_DIRECTORIES);
    },
    clearDirectories(state) {
      state.directories = [];
      state.currentDirectory = "";
    },
    resetCodeTools(state) {
      Object.assign(state, initialState);
    },
  },
});

export const {
  setSelectedCliTool,
  setSelectedTerminal,
  setSelectedModel,
  setEnvironmentVariables,
  addDirectory,
  removeDirectory,
  setCurrentDirectory,
  clearDirectories,
  resetCodeTools,
} = codeToolsSlice.actions;

export default codeToolsSlice.reducer;
