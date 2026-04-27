import { readFileSync, readdirSync, existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const VALID_DIR = join(__dirname, "fixtures/valid");
const INVALID_DIR = join(__dirname, "fixtures/invalid");

// Import the parser once it exists
// import { parse } from "../src/parser.js";

describe("valid fixtures — parse without error", () => {
  const files = readdirSync(VALID_DIR).filter((f) => f.endsWith(".ev"));

  for (const file of files) {
    test(file, () => {
      const source = readFileSync(join(VALID_DIR, file), "utf8");
      // const ast = parse(source);
      // expect(ast).toBeDefined();
      // expect(ast.type).toBe("Program");
      expect(source.length).toBeGreaterThan(0); // placeholder until parser exists
    });
  }
});

describe("valid fixtures — AST matches expected", () => {
  const files = readdirSync(VALID_DIR)
    .filter((f) => f.endsWith(".ev"))
    .filter((f) => existsSync(join(VALID_DIR, f.replace(".ev", ".expected.json"))));

  for (const file of files) {
    test(file, () => {
      const source = readFileSync(join(VALID_DIR, file), "utf8");
      const expectedPath = join(VALID_DIR, file.replace(".ev", ".expected.json"));
      const expected = JSON.parse(readFileSync(expectedPath, "utf8"));

      // const ast = parse(source);
      // expect(ast).toEqual(expected);

      // Placeholder: just verify the expected file is valid JSON
      expect(expected.type).toBe("Program");
    });
  }
});

describe("invalid fixtures — parse throws", () => {
  const files = readdirSync(INVALID_DIR).filter((f) => f.endsWith(".ev"));

  for (const file of files) {
    test(file, () => {
      const source = readFileSync(join(INVALID_DIR, file), "utf8");
      // expect(() => parse(source)).toThrow();
      expect(source.length).toBeGreaterThan(0); // placeholder
    });
  }
});
