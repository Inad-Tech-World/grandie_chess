import { createTheme, type MantineColorsTuple } from "@mantine/core";

// Grandie red, taken from the armour in the logo. Used sparingly: primary
// actions and the active navigation item. Evaluation colours stay green/red
// by meaning, not brand.
const grandie: MantineColorsTuple = [
  "#ffe9eb",
  "#fed2d5",
  "#f6a3a9",
  "#ef717a",
  "#e94752",
  "#e52e3a",
  "#e4202d",
  "#c91320",
  "#b40a1b",
  "#9e0015",
];

// Cool steel greys, like the king on the logo.
const steel: MantineColorsTuple = [
  "#f3f4f6",
  "#e6e7ea",
  "#c9ccd2",
  "#aaafb8",
  "#8f95a1",
  "#7e8592",
  "#757d8b",
  "#636a78",
  "#575e6c",
  "#484f5e",
];

export const theme = createTheme({
  primaryColor: "grandie",
  primaryShade: { light: 7, dark: 6 },
  colors: { grandie, steel },
  fontFamily:
    '-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", "Helvetica Neue", Helvetica, Arial, sans-serif',
  fontFamilyMonospace: '"SF Mono", ui-monospace, Menlo, Monaco, monospace',
  headings: {
    fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Display", "Helvetica Neue", sans-serif',
    fontWeight: "650",
  },
  defaultRadius: "md",
  cursorType: "pointer",
  components: {
    Button: { defaultProps: { radius: "md" } },
    Tooltip: { defaultProps: { openDelay: 400, withArrow: true } },
  },
});
