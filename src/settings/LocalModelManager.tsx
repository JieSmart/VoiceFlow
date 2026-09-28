import { useEffect, useState } from "react";
import {
  Check,
  Cloud,
  Cpu,
  DownloadSimple,
  HardDrive,
  Pause,
  Play,
  Trash,
} from "@phosphor-icons/react";
import clsx from "clsx";
import { contextMenuProps, copyMenuItems } from "../ui/contextMenu";
import {
  deleteLocalModel,
  downloadLocalModel,
  listLocalModels,
  localModelsDirectory,
  onEvent,
  pauseLocalModelDownload,
} from "../api";
import { useAppStore } from "../store";
import type { AppError, LocalModelInfo, LocalModelProgress } from "../types";
import { formatBytes } from "../ui/primitives";

export function LocalModelManager({
  engine,
}: {
  engine: "qwen3-asr" | "faster-whisper" | "sherpa-streaming";
}) {
  const settings = useAppStore((s) => s.settings),
    patch = useAppStore((s) => s.patchSettings);
  const [models, setModels] = useState<LocalModelInfo[]>([]),
    [directory, setDirectory] = useState(""),
    [notice, setNotice] = useState("");

  useEffect(() => {
    let disposed = false;
    void listLocalModels().then((items) => !disposed && setModels(items));
    void localModelsDirectory().then((path) => !disposed && setDirectory(path));
    const listener = onEvent<LocalModelProgress>("local-model-progress", (event) => {
      if (disposed) return;
      setModels((items) =>
        items.map((model) =>
          model.id === event.id ? { ...model, ...event } : model,
        ),
      );
      if (event.status === "done") setNotice(`${event.id} 下载完成`);
      if (event.status === "error") setNotice(event.message || "模型下载失败");
    });
    return () => {
      disposed = true;
      void listener.then((unlisten) => unlisten());
    };
  }, []);

  const act = async (action: () => Promise<unknown>, errorText: string) => {
    setNotice("");
    try {
      await action();
    } catch (error) {
      setNotice((error as AppError)?.message || errorText);
    }
  };
  const visible = models.filter((model) => model.engine === engine);
  return (
    <div className="settings-card spaced model-manager">
      <div className="model-manager-head">
        <div>
          <b>本地模型下载</b>
          <small>模型与共享推理运行时均由应用自动下载</small>
        </div>
        <span className="chip">{visible.length} 个可用模型</span>
      </div>
      <div className="model-list">
        {visible.map((model) => {
          const selected =
            settings.localModelId === model.id &&
            settings.localModelEngine === engine;
          return (
            <div className="model-row" key={model.id}>
              <button
                className={clsx("model-select", selected && "active")}
                disabled={model.status !== "done"}
                title={model.status === "done" ? "设为默认模型" : "请先下载模型"}
                onClick={() =>
                  patch({
                    localModelId: model.id,
                    localModelPath: model.path,
                    localModelEngine: engine,
                  })
                }
              >
                {selected ? <Check /> : null}
              </button>
              <div className="model-copy">
                <div>
                  <b>{model.name}</b>
                  <span className="chip">{model.size}</span>
                  <span className="chip">{model.languages}</span>
                  {selected && <span className="chip ai">默认</span>}
                </div>
                <small>{model.description} · 推理速度：{model.speed}</small>
                {(model.status === "downloading" || model.status === "paused") && (
                  <>
                    <div className="model-progress">
                      <div><i style={{ width: `${model.progress}%` }} /></div>
                      <span>{model.status === "paused" ? "已暂停" : `${model.progress}%`}</span>
                    </div>
                    {model.downloadedBytes !== undefined && model.totalBytes !== undefined && (
                      <small>
                        {formatBytes(model.downloadedBytes)} / {formatBytes(model.totalBytes)}
                        {model.status === "downloading" && model.bytesPerSecond !== undefined
                          ? ` · ${formatBytes(model.bytesPerSecond)}/s`
                          : ""}
                      </small>
                    )}
                  </>
                )}
                {model.status === "downloading" && model.message && (
                  <small>{model.message}</small>
                )}
                {model.status === "error" && (
                  <small className="error">{model.message || "下载失败，可继续重试"}</small>
                )}
              </div>
              <div className="model-actions">
                {(model.status === "idle" || model.status === "error") && (
                  <button
                    className="button small primary"
                    onClick={() =>
                      void act(() => downloadLocalModel(model.id), "无法开始下载")
                    }
                  >
                    <DownloadSimple />下载
                  </button>
                )}
                {model.status === "downloading" && (
                  <button
                    className="button small"
                    onClick={() =>
                      void act(() => pauseLocalModelDownload(model.id), "无法暂停下载")
                    }
                  >
                    <Pause />暂停
                  </button>
                )}
                {model.status === "paused" && (
                  <button
                    className="button small primary"
                    onClick={() =>
                      void act(() => downloadLocalModel(model.id), "无法继续下载")
                    }
                  >
                    <Play />继续
                  </button>
                )}
                {model.status === "done" && !selected && (
                  <button
                    className="button small"
                    onClick={() =>
                      patch({
                        localModelId: model.id,
                        localModelPath: model.path,
                        localModelEngine: engine,
                      })
                    }
                  >设为默认</button>
                )}
                {(model.status === "done" || model.status === "paused") && (
                  <button
                    className="button small"
                    onClick={() =>
                      void act(async () => {
                        await deleteLocalModel(model.id);
                        setModels((items) => items.map((item) => item.id === model.id ? { ...item, status: "idle", progress: 0, path: undefined } : item));
                        if (selected)
                          patch({
                            localModelId: undefined,
                            localModelPath: undefined,
                            localModelEngine: undefined,
                          });
                      }, "无法删除模型")
                    }
                  >
                    <Trash />删除
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
      <div className="model-directory">
        <span>存储位置</span>
        <code
          {...contextMenuProps(
            "model-directory",
            copyMenuItems([{ id: "copy-path", label: "复制路径", text: directory }]),
          )}
        >
          {directory}
        </code>
        {notice && <small className={notice.includes("失败") ? "error" : "success"}>{notice}</small>}
      </div>
    </div>
  );
}
