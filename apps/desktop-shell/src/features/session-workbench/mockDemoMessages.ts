import type { ConversationMessage } from "./types";

/**
 * Mock demo messages showcasing all UI components:
 * - User text message
 * - Assistant markdown with code blocks, tables, lists
 * - Tool use (Bash, Read, Edit, Grep, Glob)
 * - Tool results (success + error)
 * - Diff output
 * - Thinking/System message
 */
export const MOCK_DEMO_MESSAGES: ConversationMessage[] = [
  {
    id: "demo-user-1",
    role: "user",
    type: "text",
    content: "Help me set up a new React component for the settings page. I need a toggle switch for dark mode with proper TypeScript types.",
    timestamp: 1,
  },
  {
    id: "demo-assistant-1",
    role: "assistant",
    type: "text",
    content: `I'll help you create a dark mode toggle component. Let me first check the existing codebase structure and then create the component.

Here's the plan:
1. **Check** existing theme setup
2. **Create** the \`DarkModeToggle\` component
3. **Integrate** it into the settings page

Let me start by reading the current theme configuration.`,
    timestamp: 2,
  },
  {
    id: "demo-tool-read",
    role: "assistant",
    type: "tool_use",
    content: '{"file_path": "src/components/ThemeProvider.tsx"}',
    timestamp: 3,
    toolUse: {
      toolName: "Read",
      toolInput: JSON.stringify({ file_path: "src/components/ThemeProvider.tsx" }, null, 2),
    },
  },
  {
    id: "demo-tool-read-result",
    role: "assistant",
    type: "tool_result",
    content: `import { createContext, useContext, useEffect, useState } from "react";

type Theme = "light" | "dark" | "system";

interface ThemeContextValue {
  theme: Theme;
  setThemeMode: (theme: Theme) => void;
  resolvedTheme: "light" | "dark";
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<Theme>("system");
  // ... 52 more lines
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}`,
    timestamp: 4,
    toolResult: {
      toolName: "Read",
      output: `import { createContext, useContext, useEffect, useState } from "react";

type Theme = "light" | "dark" | "system";

interface ThemeContextValue {
  theme: Theme;
  setThemeMode: (theme: Theme) => void;
  resolvedTheme: "light" | "dark";
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<Theme>("system");
  // ... 52 more lines
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}`,
      isError: false,
    },
  },
  {
    id: "demo-tool-grep",
    role: "assistant",
    type: "tool_use",
    content: '{"pattern": "DarkMode|dark.?mode|theme.?toggle", "glob": "**/*.tsx"}',
    timestamp: 5,
    toolUse: {
      toolName: "Grep",
      toolInput: JSON.stringify({ pattern: "DarkMode|dark.?mode|theme.?toggle", glob: "**/*.tsx" }, null, 2),
    },
  },
  {
    id: "demo-tool-grep-result",
    role: "assistant",
    type: "tool_result",
    content: `src/shell/TabBar.tsx:186: const cycleTheme = () => {
src/components/ThemeProvider.tsx:3: type Theme = "light" | "dark" | "system";
src/features/settings/sections/GeneralSettings.tsx:45: // Theme selector`,
    timestamp: 6,
    toolResult: {
      toolName: "Grep",
      output: `src/shell/TabBar.tsx:186: const cycleTheme = () => {
src/components/ThemeProvider.tsx:3: type Theme = "light" | "dark" | "system";
src/features/settings/sections/GeneralSettings.tsx:45: // Theme selector`,
      isError: false,
    },
  },
  {
    id: "demo-assistant-2",
    role: "assistant",
    type: "text",
    content: `Good, I can see the theme system uses a \`ThemeProvider\` with three modes. Now let me create the toggle component.

The component will:
- Use the existing \`useTheme()\` hook
- Show a segmented control with **Light**, **Dark**, and **System** options
- Include proper \`aria-label\` attributes for accessibility

\`\`\`typescript
interface DarkModeToggleProps {
  className?: string;
  size?: "sm" | "md";
}
\`\`\`

Here's the implementation:`,
    timestamp: 7,
  },
  {
    id: "demo-tool-write",
    role: "assistant",
    type: "tool_use",
    content: '{"file_path": "src/components/DarkModeToggle.tsx", "content": "...component code..."}',
    timestamp: 8,
    toolUse: {
      toolName: "Write",
      toolInput: JSON.stringify({
        file_path: "src/components/DarkModeToggle.tsx",
        content: "import { Sun, Moon, Monitor } from 'lucide-react';\nimport { useTheme } from './ThemeProvider';\nimport { cn } from '@/lib/utils';\n\n// ... full component implementation",
      }, null, 2),
    },
  },
  {
    id: "demo-tool-write-result",
    role: "assistant",
    type: "tool_result",
    content: "File created successfully at: src/components/DarkModeToggle.tsx",
    timestamp: 9,
    toolResult: {
      toolName: "Write",
      output: "File created successfully at: src/components/DarkModeToggle.tsx",
      isError: false,
    },
  },
  {
    id: "demo-tool-edit",
    role: "assistant",
    type: "tool_use",
    content: '{"file_path": "src/features/settings/sections/GeneralSettings.tsx", "old_string": "// Theme selector", "new_string": "<DarkModeToggle />"}',
    timestamp: 10,
    toolUse: {
      toolName: "Edit",
      toolInput: JSON.stringify({
        file_path: "src/features/settings/sections/GeneralSettings.tsx",
        old_string: "// Theme selector",
        new_string: "<DarkModeToggle />",
      }, null, 2),
    },
  },
  {
    id: "demo-tool-edit-result",
    role: "assistant",
    type: "tool_result",
    content: `--- a/src/features/settings/sections/GeneralSettings.tsx
+++ b/src/features/settings/sections/GeneralSettings.tsx
@@ -42,7 +42,7 @@
   return (
     <div className="space-y-6">
       <SettingGroup title="Appearance">
-        // Theme selector
+        <DarkModeToggle />
       </SettingGroup>
     </div>
   );`,
    timestamp: 11,
    toolResult: {
      toolName: "Edit",
      output: `--- a/src/features/settings/sections/GeneralSettings.tsx
+++ b/src/features/settings/sections/GeneralSettings.tsx
@@ -42,7 +42,7 @@
   return (
     <div className="space-y-6">
       <SettingGroup title="Appearance">
-        // Theme selector
+        <DarkModeToggle />
       </SettingGroup>
     </div>
   );`,
      isError: false,
    },
  },
  {
    id: "demo-tool-bash",
    role: "assistant",
    type: "tool_use",
    content: '{"command": "npx tsc --noEmit 2>&1 | head -5"}',
    timestamp: 12,
    toolUse: {
      toolName: "Bash",
      toolInput: JSON.stringify({ command: "npx tsc --noEmit 2>&1 | head -5" }, null, 2),
    },
  },
  {
    id: "demo-tool-bash-result",
    role: "assistant",
    type: "tool_result",
    content: "(no output — compilation successful)",
    timestamp: 13,
    toolResult: {
      toolName: "Bash",
      output: "(no output — compilation successful)",
      isError: false,
    },
  },
  {
    id: "demo-assistant-final",
    role: "assistant",
    type: "text",
    content: `Done! The \`DarkModeToggle\` component is created and integrated. Here's a summary:

| File | Action |
|------|--------|
| \`src/components/DarkModeToggle.tsx\` | Created new component |
| \`src/features/settings/GeneralSettings.tsx\` | Added toggle to Appearance section |

The component supports two sizes (\`sm\` and \`md\`) and uses CSS variables from your Warwolf theme for consistent styling. TypeScript compilation passes with zero errors.

> **Note**: The toggle respects the system preference when set to "System" mode and will automatically update when the OS theme changes.`,
    timestamp: 14,
  },
  // ─── Error scenario: failed tool execution ──────────────────
  {
    id: "demo-tool-error-use",
    role: "assistant",
    type: "tool_use",
    content: JSON.stringify({ command: "npm run test -- --coverage" }),
    timestamp: 15,
    toolUse: {
      toolName: "Bash",
      toolInput: JSON.stringify({ command: "npm run test -- --coverage" }),
    },
  },
  {
    id: "demo-tool-error-result",
    role: "assistant",
    type: "tool_result",
    content: `FAIL src/components/DarkModeToggle.test.tsx
  ● DarkModeToggle › should toggle theme on click

    TypeError: Cannot read properties of undefined (reading 'matches')

      at Object.matchMedia (src/test/setup.ts:12:15)
      at DarkModeToggle (src/components/DarkModeToggle.tsx:18:34)

Test Suites: 1 failed, 4 passed, 5 total
Tests:       1 failed, 12 passed, 13 total`,
    timestamp: 16,
    toolResult: {
      toolName: "Bash",
      output: `FAIL src/components/DarkModeToggle.test.tsx
  ● DarkModeToggle › should toggle theme on click

    TypeError: Cannot read properties of undefined (reading 'matches')

      at Object.matchMedia (src/test/setup.ts:12:15)
      at DarkModeToggle (src/components/DarkModeToggle.tsx:18:34)

Test Suites: 1 failed, 4 passed, 5 total
Tests:       1 failed, 12 passed, 13 total`,
      isError: true,
    },
  },
  {
    id: "demo-assistant-error-recovery",
    role: "assistant",
    type: "text",
    content: `The test failed because \`window.matchMedia\` isn't available in the test environment. I'll fix the test setup by adding a mock:`,
    timestamp: 17,
  },
];
