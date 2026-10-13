import { useCallback, useRef, useState } from "react";
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
import { Tree, type NodeRendererProps } from "react-arborist";
import { DockviewReact, type DockviewReadyEvent, type DockviewApi } from "dockview-react";
import { useResizeObserver } from "@mantine/hooks";
import { useEngine } from "../context";
import type { SceneNode, PropertyValue } from "../bridge";
import { Window } from "./Window";
import { FontMarket } from "./FontMarket";

function Explorer() {
  const { snapshot, command } = useEngine();
  const [ref, rect] = useResizeObserver();
  return (
    <div ref={ref} className="inspector-panel">
      <Tree<SceneNode>
        data={snapshot.devtools?.tree ? [snapshot.devtools.tree] : []}
        width={Math.max(200, rect.width - 16)}
        height={Math.max(100, rect.height - 16)}
        rowHeight={28}
        disableDrag
        disableDrop
        disableEdit
        openByDefault
        onSelect={(nodes) =>
          void command(
            "devtools.selection",
            nodes.map((node) => node.id),
          )
        }
      >
        {({ node, tree, style }: NodeRendererProps<SceneNode>) => (
          <div
            style={style}
            className={`tree-row ${snapshot.devtools?.selected.includes(node.id) ? "selected" : ""}`}
            onClick={(event) => node.handleClick(event)}
          >
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
        )}
      </Tree>
    </div>
  );
}

function PropertyEditor({
  path,
  value,
  mixed,
  disabled,
}: {
  path: string;
  value: PropertyValue;
  mixed: boolean;
  disabled: boolean;
}) {
  const { command } = useEngine();
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
    />
  );
}
function TextDraft({
  path,
  value,
  json,
  disabled,
  mixed,
}: {
  path: string;
  value: string;
  json: boolean;
  disabled: boolean;
  mixed: boolean;
}) {
  const { command } = useEngine();
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
function Properties() {
  const { snapshot } = useEngine();
  return (
    <div className="inspector-panel">
      {!snapshot.devtools?.selected.length ? (
        <Text c="dimmed">Select an instance in Explorer.</Text>
      ) : (
        snapshot.devtools.properties?.map(({ property, mixed }) => (
          <div className="property-row" key={property.path}>
            <Text size="xs" title={property.path}>
              {property.path}
            </Text>
            <PropertyEditor
              path={property.path}
              value={property.value}
              mixed={mixed}
              disabled={!property.editable}
            />
          </div>
        ))
      )}
    </div>
  );
}
const panels = { explorer: Explorer, properties: Properties, fonts: FontMarket };
function addPanels(api: DockviewApi) {
  api.addPanel({ id: "explorer", component: "explorer", title: "Explorer" });
  api.addPanel({
    id: "properties",
    component: "properties",
    title: "Properties",
    position: { referencePanel: "explorer", direction: "right" },
  });
  api.addPanel({
    id: "fonts",
    component: "fonts",
    title: "Font Market",
    position: { referencePanel: "properties", direction: "within" },
  });
}
export function Devtools() {
  const { snapshot, command } = useEngine();
  const api = useRef<DockviewApi>(null);
  const ready = useCallback((event: DockviewReadyEvent) => {
    api.current = event.api;
    addPanels(event.api);
  }, []);
  if (!snapshot.devtools) return null;
  if (!snapshot.devtools.visible)
    return (
      <Text className="development-notice">Development build: Press F12 to enable DevTools</Text>
    );
  return (
    <Window
      title="DevTools"
      width={980}
      height={620}
      className="devtools-window"
      onClose={() => void command("devtools.visible", false)}
    >
      <Group className="devtools-toolbar" gap="xs">
        <Button
          leftSection={<ArrowCounterClockwiseIcon />}
          onClick={() => {
            api.current?.clear();
            if (api.current) addPanels(api.current);
          }}
        >
          Reset layout
        </Button>
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
        <Text>{snapshot.devtools.selected.length} selected</Text>
        <Button
          leftSection={<TrashIcon />}
          disabled={!snapshot.devtools.selected.length}
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
  );
}
