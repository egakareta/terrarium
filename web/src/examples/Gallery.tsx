import { useState } from "react";
import { Anchor, Button, Modal } from "@mantine/core";
import { assetBase } from "../routing";
import { widgetNames } from "./Widgets";

const examples = [
  ["app", "the main example"],
  ["gltf", "a custom 3d model"],
  ["outlines", "outline a mesh easily"],
  ["physics", "realistic physics with rapier"],
  ["simple", "the smallest terrarium scene: one cube"],
  ["viewport", "render scenes into textures and display them in React"],
  ["bevy", "animate a scene with Bevy systems"],
  ["sound", "spatial audio with a moving emitter"],
  ["headless", "run a scene until it requests shutdown"],
  ["render_benchmark", "measure rendering performance"],
  ["widgets", "the React component examples"],
];

export function Gallery() {
  const [active, setActive] = useState<{ name: string; url: string }>();
  const close = () => setActive(undefined);
  return (
    <div className="examples-hub">
      <div className="examples-hub-content">
        <h1>Terrarium examples 🌍</h1>
        <p>
          Open an example to try it here. Click inside the preview, then use WASD and left-click
          drag to move the camera. Use the link in the top bar to open the full-size app.
        </p>
        <main className="examples">
          {examples.map(([name, description]) => {
            const url = new URL(name + "/", assetBase()).href;
            return (
              <article className="example" key={name}>
                <div className="example-info">
                  <h2>{name}</h2>
                  <p>{description}</p>
                  <Anchor
                    unstyled
                    href={url}
                    aria-haspopup="dialog"
                    onClick={(event) => {
                      if (
                        event.button ||
                        event.metaKey ||
                        event.ctrlKey ||
                        event.shiftKey ||
                        event.altKey
                      )
                        return;
                      event.preventDefault();
                      setActive({ name, url });
                    }}
                  >
                    open {name}
                  </Anchor>
                </div>
              </article>
            );
          })}
        </main>
        <section className="examples-widgets" aria-label="Widget examples">
          <h2>Widgets</h2>
          <div className="examples">
            {widgetNames.map((name) => (
              <article className="example" key={name}>
                <div className="example-info">
                  <h2>{name.replaceAll("_", " ")}</h2>
                  <Anchor unstyled href={new URL("?widget=" + name, assetBase()).href}>
                    open {name.replaceAll("_", " ")}
                  </Anchor>
                </div>
              </article>
            ))}
          </div>
        </section>
        <footer>
          <p>
            source code is available in the{" "}
            <Anchor unstyled href="https://github.com/egakareta/terrarium/tree/master/examples">
              examples directory
            </Anchor>
          </p>
        </footer>
      </div>
      <Modal
        fullScreen
        opened={!!active}
        onClose={close}
        withCloseButton={false}
        classNames={{ content: "example-modal", body: "example-modal-body" }}
        aria-label={active?.name + " example"}
      >
        <div className="modal-toolbar">
          <h2>{active?.name}</h2>
          <Anchor unstyled href={active?.url} target="_blank" rel="noopener noreferrer">
            open full-size app
          </Anchor>
          <Button unstyled onClick={close} aria-label="Close example" data-autofocus>
            close
          </Button>
        </div>
        {active && (
          <iframe
            src={active.url}
            title={active.name + " example"}
            allowFullScreen
            onLoad={(event) => {
              event.currentTarget.contentDocument?.addEventListener(
                "keydown",
                (event) => {
                  if (event.key === "Escape") {
                    event.preventDefault();
                    event.stopPropagation();
                    close();
                  }
                },
                true,
              );
            }}
          />
        )}
      </Modal>
    </div>
  );
}
