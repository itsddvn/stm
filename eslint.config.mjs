import eslint from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist/**", "node_modules/**", "target/**", "**/*.tsbuildinfo"],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  reactHooks.configs.flat.recommended,
  {
    files: ["src/**/*.{ts,tsx}", "contracts/**/*.ts", "scripts/**/*.ts", "*.ts"],
    rules: {
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
    },
  },
  {
    files: [
      "src/components/source-install-dialog.tsx",
      "src/features/skills/skill-review-dialog.tsx",
      "src/features/tools/tool-operation-dialog.tsx",
      "src/features/updates/update-review-dialog.tsx",
      "src/features/updates/updates-page.tsx",
    ],
    rules: {
      "react-hooks/set-state-in-effect": "off",
    },
  },
  {
    files: ["src/features/updates/updates-page.tsx"],
    rules: {
      "no-unused-expressions": "off",
      "@typescript-eslint/no-unused-expressions": "off",
    },
  },
);
