import { useCallback, useMemo, useState } from "react";
import {
  ActionIcon,
  Button,
  Checkbox,
  Divider,
  Group,
  Paper,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { Tree, type NodeRendererProps } from "react-arborist";
import {
  CubeIcon,
  FolderIcon,
  CaretDownIcon,
  CaretRightIcon,
  HouseIcon,
  MagnifyingGlassIcon,
  GearIcon,
  FloppyDiskIcon,
  PlusIcon,
  TrashIcon,
  ArrowClockwiseIcon,
} from "@phosphor-icons/react";

export const widgetNames = [
  "text_outline_basic",
  "taffy_responsive",
  "taffy_grid",
  "taffy_flex_wrap",
  "taffy_flex_row",
  "taffy_flex_column",
  "taffy_egui_widgets",
  "phosphor_toolbar",
  "phosphor_subset",
  "phosphor_basic",
  "ltreeview_selection",
  "ltreeview_custom_nodes",
  "ltreeview_basic",
];
interface DemoNode {
  id: string;
  name: string;
  children?: DemoNode[];
}
function TreeDemo({ name }: { name: string }) {
  const [selected, setSelected] = useState<string[]>([]);
  const children = useMemo<DemoNode[]>(
    () =>
      name.endsWith("custom_nodes")
        ? [{ id: "1", name: "Custom leaf node" }]
        : name.endsWith("selection")
          ? ["Camera", "Light", "Ground", "Props"].map((name, i) => ({ id: String(i + 1), name }))
          : [
              {
                id: "1",
                name: "Scenes",
                children: [
                  { id: "2", name: "Main scene" },
                  { id: "3", name: "Preview scene" },
                ],
              },
              {
                id: "4",
                name: "Assets",
                children: [
                  { id: "5", name: "Materials" },
                  { id: "6", name: "Textures" },
                ],
              },
            ],
    [name],
  );
  const data = useMemo(() => [{ id: "0", name: "Workspace", children }], [children]);
  const select = useCallback(
    (nodes: { id: string }[]) => setSelected(nodes.map((node) => node.id)),
    [],
  );
  const renderNode = useCallback(
    ({ node, style }: NodeRendererProps<DemoNode>) => (
      <div style={style} className={`tree-row ${node.isSelected ? "selected" : ""}`}>
        <ActionIcon
          size="xs"
          variant="subtle"
          aria-label={`Toggle ${node.data.name}`}
          onClick={(event) => {
            event.stopPropagation();
            node.toggle();
          }}
        >
          {node.isLeaf ? null : node.isOpen ? <CaretDownIcon /> : <CaretRightIcon />}
        </ActionIcon>
        {name.endsWith("custom_nodes") ? node.isLeaf ? <CubeIcon /> : <FolderIcon /> : null}
        <span>{node.data.name}</span>
      </div>
    ),
    [name],
  );
  return (
    <Stack>
      {name.endsWith("selection") && (
        <Text>Click to select; Shift-click for a range; Ctrl/Command-click to toggle.</Text>
      )}
      <Tree<DemoNode>
        data={data}
        width={440}
        height={220}
        rowHeight={30}
        onSelect={select}
        disableDrag
        disableDrop
        openByDefault
      >
        {renderNode}
      </Tree>
      <Divider />
      <Text>Selected node IDs: [{selected.join(", ")}]</Text>
    </Stack>
  );
}
function LayoutDemo({ name }: { name: string }) {
  const wide = useMediaQuery("(min-width: 620px)");
  if (name.endsWith("flex_column"))
    return (
      <Stack gap={8} style={{ height: "70vh" }}>
        <Title order={3}>Scene hierarchy</Title>
        <Paper withBorder p="md" style={{ flex: 1 }}>
          {["Camera", "Key light", "Ground", "Props"].map((item) => (
            <Text key={item}>{item}</Text>
          ))}
        </Paper>
        <Divider />
        <Text>Status: ready</Text>
      </Stack>
    );
  if (name.endsWith("responsive"))
    return (
      <Stack>
        <Text>{wide ? "Wide layout: row" : "Narrow layout: column"}</Text>
        <div style={{ display: "flex", flexDirection: wide ? "row" : "column", gap: 12 }}>
          <Paper withBorder p="md">
            <Title order={3}>Navigation</Title>
            <Text>Scene tree</Text>
            <Text>Assets</Text>
          </Paper>
          <Paper withBorder p="md" style={{ flex: 1 }}>
            <Title order={3}>Content</Title>
            <Text>The layout direction follows available width.</Text>
          </Paper>
        </div>
      </Stack>
    );
  if (name.endsWith("grid"))
    return (
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, 180px)",
          gridAutoRows: "72px",
          gap: 8,
        }}
      >
        {Array.from({ length: 12 }, (_, cell) => (
          <Paper key={cell} withBorder p="xs">
            <Title order={3}>Cell {cell}</Title>
            <Text>Grid item</Text>
          </Paper>
        ))}
      </div>
    );
  if (name.endsWith("flex_wrap"))
    return (
      <Group gap={8}>
        {Array.from({ length: 18 }, (_, i) => (
          <Paper key={i} withBorder p="xs" style={{ width: 120, height: 56 }}>
            <Text>Item {i + 1}</Text>
          </Paper>
        ))}
      </Group>
    );
  return (
    <Group gap={12} align="stretch" wrap="nowrap">
      {["Explorer", "Viewport", "Inspector"].map((label) => (
        <Paper key={label} withBorder p="md" style={{ flex: 1 }}>
          <Title order={3}>{label}</Title>
          <Text>A flex item sized by its parent layout.</Text>
        </Paper>
      ))}
    </Group>
  );
}
export function WidgetGallery({ name }: { name: string }) {
  const [enabled, setEnabled] = useState(true),
    [action, setAction] = useState(false);
  if (!widgetNames.includes(name)) return <Text role="alert">Unknown widget example: {name}</Text>;
  return (
    <div className="widget-gallery">
      <Stack>
        <Title order={2}>{name.replaceAll("_", " ")}</Title>
        {name.startsWith("ltreeview") ? (
          <TreeDemo name={name} />
        ) : name === "phosphor_toolbar" ? (
          <>
            <Group>
              {[
                [PlusIcon, "New"],
                [FloppyDiskIcon, "Save"],
                [TrashIcon, "Delete"],
              ].map(([Icon, label]) => {
                const Component = Icon as typeof PlusIcon;
                return (
                  <Button
                    key={String(label)}
                    leftSection={<Component />}
                    onClick={() => setAction(true)}
                  >
                    {String(label)}
                  </Button>
                );
              })}
              {action && <Text>Toolbar action selected</Text>}
            </Group>
            <Divider />
            <Text>Use Phosphor icons in ordinary controls.</Text>
          </>
        ) : name.startsWith("phosphor") ? (
          <>
            <Title order={3}>Mix icons and text</Title>
            {[
              [HouseIcon, "Home"],
              [GearIcon, "Settings"],
              [
                name === "phosphor_subset" ? ArrowClockwiseIcon : MagnifyingGlassIcon,
                name === "phosphor_subset" ? "Refresh" : "Search",
              ],
            ].map(([Icon, label]) => {
              const Component = Icon as typeof HouseIcon;
              return (
                <Group key={String(label)} gap="xs">
                  <Component />
                  <Text>{String(label)}</Text>
                </Group>
              );
            })}
          </>
        ) : name.startsWith("text_outline") ? (
          <>
            <Title order={3}>Outlined text widget</Title>
            <Text
              size="32px"
              style={{
                color: "white",
                WebkitTextStroke: "2px rgb(42,160,150)",
                paintOrder: "stroke fill",
              }}
            >
              Hello from Terrarium
            </Text>
          </>
        ) : name === "taffy_egui_widgets" ? (
          <Stack gap={10}>
            <Text>A normal UI inside a flex node:</Text>
            <Checkbox
              label="Enabled"
              checked={enabled}
              onChange={(e) => setEnabled(e.currentTarget.checked)}
            />
            <Button disabled={!enabled}>A widget leaf</Button>
            <Paper withBorder p="md">
              A nested flex container
            </Paper>
          </Stack>
        ) : (
          <LayoutDemo name={name} />
        )}
      </Stack>
    </div>
  );
}
