import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

const sourceFiles = ["src/**/*.ts", "vitest.config.ts"];

export default tseslint.config(
  { ignores: ["dist/**", "artifacts/**", "node_modules/**"] },
  { ...js.configs.recommended, files: sourceFiles },
  ...tseslint.configs.recommendedTypeChecked.map((configuration) => ({ ...configuration, files: sourceFiles })),
  {
    files: sourceFiles,
    languageOptions: {
      parserOptions: { project: "./tsconfig.json", tsconfigRootDir: import.meta.dirname },
      globals: { ...globals.browser, ...globals.webextensions, browser: "readonly" },
    },
    rules: {
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      "@typescript-eslint/no-unnecessary-condition": "off",
      "@typescript-eslint/no-explicit-any": "error",
      "no-console": ["error", { allow: ["warn", "error"] }],
    },
  },
  {
    ...js.configs.recommended,
    files: ["scripts/**/*.mjs", "eslint.config.js", "web-ext-config.mjs"],
    languageOptions: { globals: globals.node },
  },
);
