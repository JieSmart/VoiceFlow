import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

export function KpiSpark({ data }: { data: number[] }) {
  return (
    <ResponsiveContainer>
      <AreaChart data={data.map((v, i) => ({ i, v }))}>
        <Area
          dataKey="v"
          stroke="#141414"
          fill="transparent"
          strokeWidth={1.7}
        />
      </AreaChart>
    </ResponsiveContainer>
  );
}

export function UsageChart({
  series,
}: {
  series: { label: string; value: number }[];
}) {
  return (
    <ResponsiveContainer>
      <BarChart data={series} margin={{ left: -22, right: 4 }}>
        <XAxis
          dataKey="label"
          tickLine={false}
          axisLine={false}
          fontSize={10}
        />
        <YAxis tickLine={false} axisLine={false} fontSize={10} />
        <Tooltip cursor={{ fill: "#f2f2f2" }} />
        <Bar dataKey="value" fill="#9a9a9a" radius={[4, 4, 1, 1]} />
      </BarChart>
    </ResponsiveContainer>
  );
}
