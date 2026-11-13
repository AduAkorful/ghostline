import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";

const binary = process.argv[2];
if (!binary) throw new Error("Pass the path to a current ghostline executable");

function describe(path) {
  const help = execFileSync(binary, [...path, "--help"], { encoding: "utf8" });
  const commands = help.match(/^Commands:\n([\s\S]+?)(?:\n\n|(?![\s\S]))/m)?.[1] ?? "";
  const children = [...commands.matchAll(/^ {2}([\w-]+) {2,}/gm)]
    .map(([, name]) => name).filter((name) => name !== "help");
  const heading = path.length ? `ghostline ${path.join(" ")}` : "ghostline";
  return [`## ${heading}\n\n\`\`\`text\n${help.trimEnd()}\n\`\`\`\n`, ...children.flatMap((name) => describe([...path, name]))];
}

const intro = "# Ghostline CLI\n\nCommand reference generated from the executable's `--help`. Use the [mainnet quickstart](mainnet-quickstart.md) for wallet setup and a full channel walkthrough.\n";
writeFileSync(new URL("../../docs/cli.md", import.meta.url), [intro, ...describe([])].join("\n"));
