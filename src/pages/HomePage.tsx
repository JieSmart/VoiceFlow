import { lazy, Suspense, useMemo, useState, type ReactNode } from "react";
import { ChartBar, Waveform } from "@phosphor-icons/react";
import { recentHotwordRecords, usageSeries, type UsageRange } from "../recordsView";
import { useAppStore } from "../store";

const KpiSpark = lazy(() =>
  import("./HomeCharts").then((module) => ({ default: module.KpiSpark })),
);
const UsageChart = lazy(() =>
  import("./HomeCharts").then((module) => ({ default: module.UsageChart })),
);

type DashboardKpi = { label: string; value: string; unit: string; data: number[] };

function SectionTitle({
  children,
  side,
}: {
  children: ReactNode;
  side?: ReactNode;
}) {
  return (
    <div className="card-head">
      <div className="card-title">
        <ChartBar size={18} weight="bold" />
        {children}
      </div>
      {side}
    </div>
  );
}

function KpiCard({ item }: { item: DashboardKpi }) {
  return (
    <div className="card kpi">
      <div className="kpi-top">
        <b>{item.label}</b>
        <span className="up">今日</span>
      </div>
      <div className="kpi-main">
        <strong>{item.value}</strong>
        <small>{item.unit}</small>
        <div className="spark">
          <Suspense fallback={null}>
            <KpiSpark data={item.data} />
          </Suspense>
        </div>
      </div>
    </div>
  );
}

export function HomePage() {
  const hotwords = useAppStore((s) => s.hotwords);
  const records = useAppStore((s) => s.records);
  const settings = useAppStore((s) => s.settings);
  const setRoute = useAppStore((s) => s.setRoute);
  const [range, setRange] = useState<UsageRange>("day");
  const series = useMemo(() => usageSeries(records, range), [records, range]);
  const recentHotwords = useMemo(() => recentHotwordRecords(records), [records]);
  const dashboard = useMemo(() => {
    const todayKey = new Date().toLocaleDateString("sv-SE");
    const today = records.filter((record) => new Date(record.startedAt).toLocaleDateString("sv-SE") === todayKey);
    const hours = Array.from({ length: 24 }, (_, hour) => ({ hour: String(hour), value: 0 }));
    for (const record of today) hours[new Date(record.startedAt).getHours()].value += record.chars;
    const spark = Array.from({ length: 12 }, (_, index) => hours[index * 2].value + hours[index * 2 + 1].value);
    const chars = today.reduce((sum, record) => sum + record.chars, 0);
    const seconds = today.reduce((sum, record) => sum + record.duration, 0);
    const hits = today.reduce((sum, record) => sum + record.hotwords.length, 0);
    const ai = today.filter((record) => record.ai).length;
    return {
      kpis: [
        { label: "今日输入字数", value: chars.toLocaleString(), unit: "字", data: spark },
        { label: "今日语音时长", value: (seconds / 60).toFixed(seconds ? 1 : 0), unit: "分钟", data: spark.map((_, i) => today.filter((r) => Math.floor(new Date(r.startedAt).getHours() / 2) === i).reduce((n, r) => n + r.duration, 0)) },
        { label: "热词命中", value: String(hits), unit: "次", data: spark.map((_, i) => today.filter((r) => Math.floor(new Date(r.startedAt).getHours() / 2) === i).reduce((n, r) => n + r.hotwords.length, 0)) },
        { label: "AI 格式化", value: String(ai), unit: "次", data: spark.map((_, i) => today.filter((r) => r.ai && Math.floor(new Date(r.startedAt).getHours() / 2) === i).length) },
      ] as DashboardKpi[],
    };
  }, [records]);
  return (
    <div className="content">
      <div className="kpi-grid">
        {dashboard.kpis.map((k) => (
          <KpiCard key={k.label} item={k} />
        ))}
      </div>
      <div className="home-grid">
        <section className="card usage">
          <SectionTitle
            side={
              <div className="segment">
                {([
                  ["day", "天"],
                  ["week", "周"],
                  ["month", "月"],
                ] as const).map(([value, label]) => (
                  <button
                    key={value}
                    className={range === value ? "active" : ""}
                    onClick={() => setRange(value)}
                  >
                    {label}
                  </button>
                ))}
              </div>
            }
          >
            使用频率统计
          </SectionTitle>
          <div className="chart-caption">
            <span>
              {range === "day"
                ? "按小时统计输入字数 · 今日"
                : range === "week"
                  ? "按天统计输入字数 · 近 7 天"
                  : "按天统计输入字数 · 近 30 天"}
            </span>
            <small>悬停查看数值</small>
          </div>
          <div className="big-chart">
            <Suspense fallback={null}>
              <UsageChart series={series} />
            </Suspense>
          </div>
        </section>
        <div className="right-stack">
          <section className="card">
            <SectionTitle side={<small>累计</small>}>
              热词命中排行
            </SectionTitle>
            <div className="rank-list">
              {[...hotwords].sort((a, b) => b.hits - a.hits).slice(0, 6).map((h, i) => (
                <div className="rank" key={h.id}>
                  <span>0{i + 1}</span>
                  <b>{h.word}</b>
                  <i>
                    <em
                      style={{
                        width: `${Math.max(4, (h.hits / Math.max(1, ...hotwords.map((item) => item.hits))) * 100)}%`,
                      }}
                    />
                  </i>
                  <small>{h.hits}</small>
                </div>
              ))}
            </div>
          </section>
          <section className="card heat-card">
            <SectionTitle side={<small>{records.length ? "真实使用记录" : "暂无记录"}</small>}>
              活跃热力图
            </SectionTitle>
            <div className="heatmap">
              {Array.from({ length: 84 }, (_, i) => {
                const day = new Date();
                day.setDate(day.getDate() - (83 - i));
                const key = day.toLocaleDateString("sv-SE");
                const count = records.filter((record) => new Date(record.startedAt).toLocaleDateString("sv-SE") === key).length;
                return <i key={i} className={`l${Math.min(4, count)}`} title={`${key} · ${count} 次`} />;
              })}
            </div>
          </section>
        </div>
      </div>
      <section className="card quick">
        <div>
          <Waveform size={23} />
          <div>
            <b>实时语音输入</b>
            <small>
              {settings.directInsert
                ? settings.autoFormat
                  ? "识别完成后经 AI 整理并直接上屏"
                  : "识别完成后直接写入原输入框"
                : "识别完成后先确认，再决定是否上屏"}
            </small>
          </div>
        </div>
        <span className="live-badge">
          <i />
          {settings.directInsert ? "直接上屏已开启" : "审核模式已开启"}
        </span>
        <span className="chip">按唤醒键开始</span>
      </section>
      <section className="card recent-hotwords">
        <SectionTitle
          side={
            <button className="link-button" onClick={() => setRoute("records")}>
              查看全部
            </button>
          }
        >
          热词响应记录
        </SectionTitle>
        <div className="recent-hotword-list">
          {recentHotwords.map((record) => (
            <button
              className="recent-hotword"
              key={record.id}
              onClick={() => setRoute("records")}
            >
              <span>
                {new Date(record.startedAt).toLocaleString("zh-CN", {
                  month: "numeric",
                  day: "numeric",
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </span>
              <div>
                {record.hotwords.slice(0, 3).map((word) => (
                  <b key={`${record.id}-${word}`}>{word}</b>
                ))}
              </div>
              <small>{record.formattedText || record.text}</small>
              <em>{record.app}</em>
            </button>
          ))}
          {!recentHotwords.length && <div className="empty">还没有热词命中记录</div>}
        </div>
      </section>
    </div>
  );
}
