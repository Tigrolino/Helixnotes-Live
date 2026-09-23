import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { createContext, Script } from 'node:vm';
import { compile, parse } from 'svelte/compiler';
import { get, writable } from 'svelte/store';

const source = await readFile(
  new URL('../src/lib/components/Sidebar.svelte', import.meta.url),
  'utf8'
);
const ast = parse(source, { modern: true });

function elementWithClass(nodes, className) {
  return nodes.find((node) => node.attributes?.some((attribute) =>
    attribute.name === 'class' && attribute.value[0]?.data === className
  ));
}

const aside = elementWithClass(ast.fragment.nodes, 'sidebar');
const blocks = aside.fragment.nodes.filter((node) => node.type === 'IfBlock');
const content = blocks.find((node) => elementWithClass(node.consequent.nodes, 'section'));
const header = blocks.find((node) => elementWithClass(node.consequent.nodes, 'sidebar-header'));
const toggle = elementWithClass(
  elementWithClass(header.consequent.nodes, 'sidebar-header').fragment.nodes,
  'collapse-btn'
);

function expressionScript(expression) {
  assert.ok(expression, 'expected a component expression');
  return new Script(`(${source.slice(expression.start, expression.end)})`);
}

// Execute the actual template conditions, not a copy of the collapse logic.
const expressions = {
  contentVisible: expressionScript(content.test),
  collapsedClass: expressionScript(aside.attributes.find((attribute) =>
    attribute.type === 'ClassDirective' && attribute.name === 'collapsed'
  ).expression),
  toggleVisible: expressionScript(header.test)
};
const toggleScript = expressionScript(
  toggle.attributes.find((attribute) => attribute.name === 'onclick').value.expression
);

function sidebarContext(isMobile, stored) {
  const sidebarCollapsed = writable(stored);
  const context = createContext({
    isMobile,
    get $sidebarCollapsed() { return get(sidebarCollapsed); },
    set $sidebarCollapsed(value) { sidebarCollapsed.set(value); }
  });
  return { context, sidebarCollapsed };
}

function evaluate(context) {
  return Object.fromEntries(Object.entries(expressions).map(([name, script]) =>
    [name, script.runInContext(context)]
  ));
}

const scenarios = [
  {
    platform: 'desktop', isMobile: false,
    expanded: { contentVisible: true, collapsedClass: false, toggleVisible: true },
    collapsed: { contentVisible: false, collapsedClass: true, toggleVisible: true }
  },
  {
    platform: 'mobile', isMobile: true,
    expanded: { contentVisible: true, collapsedClass: false, toggleVisible: false },
    collapsed: { contentVisible: true, collapsedClass: false, toggleVisible: false }
  }
];

for (const scenario of scenarios) {
  for (const stored of [false, true]) {
    test(`${scenario.platform} navigation with stored sidebar_collapsed=${stored}`, () => {
      const { context, sidebarCollapsed } = sidebarContext(scenario.isMobile, stored);

      assert.deepEqual(evaluate(context), stored ? scenario.collapsed : scenario.expanded);
      assert.equal(get(sidebarCollapsed), stored, 'rendering must preserve the stored preference');
    });
  }

  test(`${scenario.platform} navigation follows late false -> true -> false store updates`, () => {
    const { context, sidebarCollapsed } = sidebarContext(scenario.isMobile, false);

    assert.deepEqual(evaluate(context), scenario.expanded);
    sidebarCollapsed.set(true);
    assert.deepEqual(evaluate(context), scenario.collapsed);
    assert.equal(get(sidebarCollapsed), true, 'rendering must not reset the loaded preference');
    sidebarCollapsed.set(false);
    assert.deepEqual(evaluate(context), scenario.expanded);
    assert.equal(get(sidebarCollapsed), false);
  });
}

test('desktop collapse toggle still collapses and expands the sidebar', () => {
  const { context, sidebarCollapsed } = sidebarContext(false, false);
  const toggleSidebar = toggleScript.runInContext(context);

  toggleSidebar();
  assert.equal(get(sidebarCollapsed), true);
  assert.deepEqual(evaluate(context), scenarios[0].collapsed);
  toggleSidebar();
  assert.equal(get(sidebarCollapsed), false);
  assert.deepEqual(evaluate(context), scenarios[0].expanded);
});

test('Sidebar compiles for the client', () => {
  assert.doesNotThrow(() => compile(source, {
    filename: 'src/lib/components/Sidebar.svelte',
    generate: 'client'
  }));
});
