import {
  createContext,
  memo,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  ActionIcon,
  Button,
  Checkbox,
  ColorInput,
  Group,
  NumberInput,
  Menu,
  Select,
  Stack,
  Text,
  Textarea,
  TextInput,
} from "@mantine/core";
import {
  CaretDownIcon,
  CaretRightIcon,
  CubeIcon,
  TrashIcon,
  ArrowCounterClockwiseIcon,
} from "@phosphor-icons/react";
import { Tree, type TreeApi, type NodeApi, type NodeRendererProps } from "react-arborist";
import { DockviewReact, type DockviewReadyEvent, type DockviewApi } from "dockview-react";
import { useResizeObserver } from "@mantine/hooks";
import { defaultRangeExtractor, useVirtualizer, type Range } from "@tanstack/react-virtual";
import { useEngine, type EngineContextValue } from "../context";
import type { SceneNode, PropertyValue, InspectorState } from "../bridge";
import { Window } from "./Window";
import { FontMarket } from "./FontMarket";

type Command = EngineContextValue["command"];
const InspectorContext = createContext<{ tools: InspectorState; command: Command } | null>(null);
function useInspector() {
  return useContext(InspectorContext)!;
}
function sameSelection(left: string[], right: string[]) {
  return left.length === right.length && left.every((id) => right.includes(id));
}
function SceneRow({ node, style }: NodeRendererProps<SceneNode>) {
  return (
    <div style={style} className={`tree-row ${node.isSelected ? "selected" : ""}`}>
      <ActionIcon
        size="xs"
        variant="transparent"
        aria-label={`${node.isOpen ? "Collapse" : "Expand"} ${node.data.name}`}
        onClick={(event) => {
          event.stopPropagation();
          node.toggle();
        }}
      >
        {node.isLeaf ? null : node.isOpen ? <CaretDownIcon /> : <CaretRightIcon />}
      </ActionIcon>
      <CubeIcon />
      <span>{node.data.name}</span>
      <Text size="xs" c="dimmed">
        {node.data.class}
      </Text>
    </div>
  );
}

function Explorer() {
  const { tools, command } = useInspector();
  const [ref, rect] = useResizeObserver();
  const tree = useRef<TreeApi<SceneNode>>(null);
  const syncing = useRef(true);
  const selected = useRef(tools.selected);
  const data = useMemo(() => (tools.tree ? [tools.tree] : []), [tools.tree]);
  const select = useCallback(
    (nodes: NodeApi<SceneNode>[]) => {
      const ids = nodes.map((node) => node.id);
      if (!syncing.current && !sameSelection(ids, selected.current)) {
        selected.current = ids;
        void command("devtools.selection", ids);
      }
    },
    [command],
  );
  useEffect(() => {
    syncing.current = true;
    selected.current = tools.selected;
    const api = tree.current;
    if (api && !sameSelection([...api.selectedIds], tools.selected)) {
      api.setSelection({
        ids: tools.selected,
        anchor: tools.selected.at(-1) ?? null,
        mostRecent: tools.selected.at(-1) ?? null,
      });
    }
    syncing.current = false;
  }, [tools.selected, data]);
  return (
    <div ref={ref} className="inspector-panel">
      <Tree<SceneNode>
        ref={tree}
        data={data}
        width={Math.max(200, rect.width - 16)}
        height={Math.max(100, rect.height - 16)}
        rowHeight={28}
        disableDrag
        disableDrop
        disableEdit
        openByDefault
        onSelect={select}
      >
        {SceneRow}
      </Tree>
    </div>
  );
}

function PropertyEditor({
  path,
  value,
  mixed,
  disabled,
  command,
}: {
  path: string;
  value: PropertyValue;
  mixed: boolean;
  disabled: boolean;
  command: Command;
}) {
  const edit = (value: PropertyValue) => void command("devtools.edit", { path, value });
  const label = path.split(".").at(-1)!;
  const placeholder = mixed ? "Mixed" : undefined;
  if (value.Bool !== undefined)
    return (
      <Checkbox
        aria-label={path}
        checked={value.Bool}
        indeterminate={mixed}
        disabled={disabled}
        onChange={(e) => edit({ Bool: e.currentTarget.checked })}
      />
    );
  if (value.Number !== undefined)
    return (
      <NumberInput
        aria-label={path}
        value={value.Number}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(v) => {
          if (typeof v === "number") edit({ Number: v });
        }}
      />
    );
  if (value.Choice)
    return (
      <Select
        aria-label={path}
        value={mixed ? null : value.Choice.selected}
        data={value.Choice.options}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(v) => {
          if (v) edit({ Choice: { ...value.Choice!, selected: v } });
        }}
      />
    );
  if (value.Color) {
    const channels = value.Color.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255));
    return (
      <ColorInput
        aria-label={path}
        value={`rgb(${channels.join(", ")})`}
        format="rgb"
        disabled={disabled}
        onChangeEnd={(v) => {
          const channels = v.match(/[\d.]+/g)?.map(Number);
          if (channels?.length === 3) edit({ Color: channels.map((v) => v / 255) });
        }}
      />
    );
  }
  if (value.Vector)
    return (
      <Group gap={4} wrap="nowrap">
        {value.Vector.map((component, index) => (
          <NumberInput
            key={index}
            aria-label={`${path} ${index}`}
            value={component}
            disabled={disabled}
            hideControls
            onChange={(v) => {
              if (typeof v === "number")
                edit({ Vector: value.Vector!.map((current, i) => (i === index ? v : current)) });
            }}
          />
        ))}
      </Group>
    );
  return (
    <TextDraft
      key={`${path}:${label}`}
      path={path}
      value={value.Json ?? value.Text ?? ""}
      json={value.Json !== undefined}
      disabled={disabled}
      mixed={mixed}
      command={command}
    />
  );
}
function TextDraft({
  path,
  value,
  json,
  disabled,
  mixed,
  command,
}: {
  path: string;
  value: string;
  json: boolean;
  disabled: boolean;
  mixed: boolean;
  command: Command;
}) {
  const [draft, setDraft] = useState<string>();
  const save = () => {
    if (draft !== undefined) {
      void command("devtools.edit", { path, value: json ? { Json: draft } : { Text: draft } });
      setDraft(undefined);
    }
  };
  const props = {
    "aria-label": path,
    value: draft ?? (mixed ? "" : value),
    disabled,
    placeholder: mixed ? "Mixed" : undefined,
    onChange: (event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
      setDraft(event.currentTarget.value),
    onBlur: save,
  };
  return json ? (
    <Textarea {...props} autosize minRows={2} />
  ) : (
    <TextInput
      {...props}
      onKeyDown={(event) => {
        if (event.key === "Enter") save();
      }}
    />
  );
}
const PropertyRow = memo(
  function PropertyRow({
    field,
    command,
  }: {
    field: NonNullable<InspectorState["properties"]>[number];
    command: Command;
  }) {
    const { property, mixed } = field;
    return (
      <div className="property-row">
        <Text size="xs" title={property.path}>
          {property.path}
        </Text>
        <PropertyEditor
          path={property.path}
          value={property.value}
          mixed={mixed}
          disabled={!property.editable}
          command={command}
        />
      </div>
    );
  },
  (previous, next) =>
    previous.command === next.command &&
    JSON.stringify(previous.field) === JSON.stringify(next.field),
);
function Properties() {
  const { tools, command } = useInspector();
  const container = useRef<HTMLDivElement>(null);
  const [focusedRow, setFocusedRow] = useState<number | null>(null);
  const fields = tools.selected.length ? (tools.properties ?? []) : [];
  const rangeExtractor = useCallback(
    (range: Range) => {
      const rows = defaultRangeExtractor(range);
      if (focusedRow !== null && focusedRow < range.count && !rows.includes(focusedRow)) {
        rows.push(focusedRow);
        rows.sort((left, right) => left - right);
      }
      return rows;
    },
    [focusedRow],
  );
  const virtual = useVirtualizer({
    count: fields.length,
    getScrollElement: () => container.current,
    estimateSize: () => 50,
    getItemKey: (index) => fields[index].property.path,
    overscan: 3,
    rangeExtractor,
  });
  return (
    <div
      ref={container}
      className="inspector-panel"
      onFocusCapture={(event) => {
        const row = (event.target as HTMLElement).closest<HTMLElement>("[data-index]");
        setFocusedRow(row ? Number(row.dataset.index) : null);
      }}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setFocusedRow(null);
      }}
    >
      {!tools.selected.length ? (
        <Text c="dimmed">Select an instance in Explorer.</Text>
      ) : (
        <div style={{ height: virtual.getTotalSize(), width: "100%", position: "relative" }}>
          {virtual.getVirtualItems().map((row) => (
            <div
              key={row.key}
              ref={virtual.measureElement}
              data-index={row.index}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${row.start}px)`,
              }}
            >
              <PropertyRow field={fields[row.index]} command={command} />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
const panels = { explorer: Explorer, properties: Properties, fonts: FontMarket };
function addPanels(api: DockviewApi) {
  api.addPanel({ id: "explorer", component: "explorer", title: "Explorer" });
  api.addPanel({
    id: "fonts",
    component: "fonts",
    title: "Font Market",
    position: { referencePanel: "explorer", direction: "within" },
    inactive: true,
  });
  api.addPanel({
    id: "properties",
    component: "properties",
    title: "Properties",
    position: { referencePanel: "explorer", direction: "below" },
  });
  api.getPanel("explorer")?.api.setActive();
  api.getPanel("explorer")?.group.api.setSize({ height: api.height * 0.55 });
}
export function Devtools() {
  const { snapshot, command } = useEngine();
  return <Inspector tools={snapshot.devtools} command={command} />;
}
const Inspector = memo(function Inspector({
  tools,
  command,
}: {
  tools: InspectorState | null;
  command: Command;
}) {
  const api = useRef<DockviewApi>(null);
  const context = useMemo(() => (tools ? { tools, command } : null), [tools, command]);
  const ready = useCallback((event: DockviewReadyEvent) => {
    api.current = event.api;
    addPanels(event.api);
  }, []);
  if (!tools) return null;
  const notice = (
    <Text className="development-notice">Development build: Press F12 to enable DevTools</Text>
  );
  if (!tools.visible) return notice;
  return (
    <InspectorContext.Provider value={context}>
      <Window
        title="DevTools"
        width={340}
        height={420}
        className="devtools-window"
        onClose={() => void command("devtools.visible", false)}
      >
        <Group className="devtools-toolbar" gap="xs">
          <Menu>
            <Menu.Target>
              <Button>Panels</Button>
            </Menu.Target>
            <Menu.Dropdown>
              {Object.keys(panels).map((id) => (
                <Menu.Item
                  key={id}
                  onClick={() => {
                    const dock = api.current;
                    if (!dock) return;
                    const panel = dock.getPanel(id);
                    if (panel) panel.api.setActive();
                    else
                      dock.addPanel({
                        id,
                        component: id,
                        title:
                          id === "fonts"
                            ? "Font Market"
                            : id === "explorer"
                              ? "Explorer"
                              : "Properties",
                      });
                  }}
                >
                  {id === "fonts" ? "Font Market" : id === "explorer" ? "Explorer" : "Properties"}
                </Menu.Item>
              ))}
            </Menu.Dropdown>
          </Menu>
          <Button
            leftSection={<ArrowCounterClockwiseIcon />}
            onClick={() => {
              api.current?.clear();
              if (api.current) addPanels(api.current);
            }}
          >
            Reset layout
          </Button>
          <Text>{tools.selected.length} selected</Text>
          <Button
            leftSection={<TrashIcon />}
            disabled={!tools.selected.length}
            onClick={() => void command("devtools.delete")}
          >
            Delete selection
          </Button>
        </Group>
        <DockviewReact
          className="devtools-dock dockview-theme-dark"
          components={panels}
          onReady={ready}
        />
      </Window>
      {notice}
    </InspectorContext.Provider>
  );
});
