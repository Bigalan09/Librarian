#!/usr/bin/env bun
/**
 * Librarian MCP Server
 *
 * Exposes Librarian CLI commands as MCP tools so LLM agents (Claude, etc.)
 * can organise files directly.
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

const server = new McpServer({
  name: "librarian",
  version: "0.1.0",
});

// --- Helpers ---

async function run(
  args: string[],
): Promise<{ success: boolean; output: string }> {
  const proc = Bun.spawn(["librarian", "--json", ...args], {
    stdout: "pipe",
    stderr: "pipe",
  });

  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;

  const output = stdout + (stderr ? `\n${stderr}` : "");
  return { success: exitCode === 0, output: output.trim() };
}

function toolResult(result: { success: boolean; output: string }) {
  return {
    content: [{ type: "text" as const, text: result.output }],
    isError: !result.success,
  };
}

// --- Tools ---

server.tool(
  "librarian_status",
  "Show current Librarian status: recent plans, pending reviews, and configuration summary",
  {},
  async () => toolResult(await run(["status"])),
);

server.tool(
  "librarian_process",
  "Scan inbox folders, classify files using rules and AI, and produce a plan. Returns the plan name for use with apply/show.",
  {
    source: z
      .array(z.string())
      .optional()
      .describe("Inbox folders to scan (defaults to config)"),
    destination: z
      .string()
      .optional()
      .describe("Target root directory (defaults to config)"),
    take: z
      .number()
      .optional()
      .describe("Only process the first N files"),
    rename: z
      .boolean()
      .optional()
      .describe("Also propose file renames"),
    plan_name: z
      .string()
      .optional()
      .describe("Custom name for the saved plan"),
  },
  async ({ source, destination, take, rename, plan_name }) => {
    const args = ["process"];
    if (source) for (const s of source) args.push("--source", s);
    if (destination) args.push("--destination", destination);
    if (take) args.push("--take", String(take));
    if (rename) args.push("--rename");
    if (plan_name) args.push("--plan-name", plan_name);
    return toolResult(await run(args));
  },
);

server.tool(
  "librarian_plans_list",
  "List all saved plans with their status, action count, and creation date",
  {},
  async () => toolResult(await run(["plans", "list"])),
);

server.tool(
  "librarian_plans_show",
  "Show details of a specific plan including all planned file moves",
  {
    name: z
      .string()
      .describe("Plan name, ID, or 'latest'"),
  },
  async ({ name }) => toolResult(await run(["plans", "show", name])),
);

server.tool(
  "librarian_apply",
  "Execute a plan, moving files to their classified destinations. Use --backup for safety.",
  {
    plan: z
      .string()
      .describe("Plan name, ID, or 'latest'"),
    backup: z
      .boolean()
      .optional()
      .describe("Create backup before applying (recommended)"),
    dry_run: z
      .boolean()
      .optional()
      .describe("Show what would happen without moving files"),
  },
  async ({ plan, backup, dry_run }) => {
    const args = ["apply", "--plan", plan];
    if (backup) args.push("--backup");
    if (dry_run) args.push("--dry-run");
    return toolResult(await run(args));
  },
);

server.tool(
  "librarian_rollback",
  "Reverse an applied plan, restoring files to their original locations",
  {
    plan: z
      .string()
      .describe("Plan name, ID, or 'latest'"),
  },
  async ({ plan }) => toolResult(await run(["rollback", "--plan", plan])),
);

server.tool(
  "librarian_correct",
  "Record a manual correction so Librarian learns from the mistake (works with files and folders)",
  {
    file: z.string().describe("Path to the incorrectly placed file or folder"),
    to: z.string().describe("Correct destination path"),
  },
  async ({ file, to }) =>
    toolResult(await run(["correct", file, "--to", to])),
);

server.tool(
  "librarian_rules_validate",
  "Validate the rules.yaml file for syntax and logic errors",
  {},
  async () => toolResult(await run(["rules", "validate"])),
);

server.tool(
  "librarian_rules_suggest",
  "Suggest new rules based on correction history (requires ≥3 similar corrections)",
  {},
  async () => toolResult(await run(["rules", "suggest"])),
);

server.tool(
  "librarian_config_show",
  "Show the current Librarian configuration",
  {},
  async () => toolResult(await run(["config", "show"])),
);

server.tool(
  "librarian_plans_delete",
  "Delete a plan by name or ID",
  {
    name: z.string().describe("Plan name, ID, or 'latest'"),
  },
  async ({ name }) => toolResult(await run(["plans", "delete", name])),
);

server.tool(
  "librarian_plans_clean",
  "Remove plans older than N days",
  {
    days: z
      .number()
      .optional()
      .describe("Max age in days (default: 30)"),
  },
  async ({ days }) => {
    const args = ["plans", "clean"];
    if (days) args.push("--days", String(days));
    return toolResult(await run(args));
  },
);

server.tool(
  "librarian_suggest_structure",
  "Use AI to suggest a folder structure and rules based on your files",
  {},
  async () => toolResult(await run(["suggest-structure"])),
);

// --- Start ---

const transport = new StdioServerTransport();
await server.connect(transport);
