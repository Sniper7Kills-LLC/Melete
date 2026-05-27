/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  // `media` mirrors the desktop's `adw::StyleManager` behaviour — the
  // OS color-scheme picks light vs dark without a manual toggle. Any
  // `dark:` variant in a class string activates when the OS prefers
  // dark.
  darkMode: "media",
  theme: {
    extend: {},
  },
  plugins: [],
};
