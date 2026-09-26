// Transform command tokens only. Paths, package names, URLs and JSON stay literal.
export function commandVariant(source, method) {
  const executable =
    {
      installed: 'selfhost',
      npx: 'npx selfhost',
      pnpx: 'pnpx selfhost',
      pnpm: 'pnpm dlx selfhost',
    }[method] || 'selfhost';
  return source.replace(
    /(^|\n)(\s*(?:Usage:\s*|\$\s*)?)(?:(?:npx|pnpx|pnpm\s+dlx)\s+)?selfhost(?:\.exe)?(?=\s|$)/g,
    (_, start, prefix) => start + prefix + executable,
  );
}
const attribute = (value) =>
  JSON.stringify(value)
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
export function cliMarkdown(md) {
  const fence = md.renderer.rules.fence;
  md.renderer.rules.fence = (tokens, index, ...args) => {
    const token = tokens[index];
    if (
      !/^(sh|bash|shell|powershell|ps1|text|console|zsh)?(?:\s|$)/.test(token.info.trim()) ||
      (commandVariant(token.content, 'npx') === token.content &&
        commandVariant(token.content, 'installed') === token.content)
    )
      return fence(tokens, index, ...args);
    const original = token.content;
    const variants = {};
    try {
      for (const method of ['installed', 'npx', 'pnpx', 'pnpm']) {
        token.content = commandVariant(original, method);
        variants[method] = fence(tokens, index, ...args);
      }
    } finally {
      token.content = original;
    }
    return `<CliCode :variants="${attribute(variants)}" />`;
  };
  const inline = md.renderer.rules.code_inline;
  md.renderer.rules.code_inline = (tokens, index, ...args) => {
    const content = tokens[index].content;
    if (!/^(?:(?:npx|pnpx|pnpm\s+dlx)\s+)?selfhost(?:\.exe)?\s+/.test(content))
      return inline(tokens, index, ...args);
    return `<CliInline :variants="${attribute(Object.fromEntries(['installed', 'npx', 'pnpx', 'pnpm'].map((method) => [method, commandVariant(content, method)])))}" />`;
  };
}
