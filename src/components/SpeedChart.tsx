import React from "react";
import { ResponsiveContainer, AreaChart, Area, XAxis, YAxis, Tooltip } from "recharts";
import { formatBytes } from "../lib/utils";
import type { TrafficPoint } from "../stores/appStore";

interface SpeedChartProps {
  data: TrafficPoint[];
}

export const SpeedChart: React.FC<SpeedChartProps> = ({ data }) => {
  return (
    <div className="w-full h-44">
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={data} margin={{ top: 10, right: 10, left: -20, bottom: 0 }}>
          <defs>
            <linearGradient id="downGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="#38bdf8" stopOpacity={0.4} />
              <stop offset="95%" stopColor="#38bdf8" stopOpacity={0.0} />
            </linearGradient>
            <linearGradient id="upGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="#34d399" stopOpacity={0.4} />
              <stop offset="95%" stopColor="#34d399" stopOpacity={0.0} />
            </linearGradient>
          </defs>
          <XAxis dataKey="time" hide />
          <YAxis
            tickFormatter={(val) => `${formatBytes(val, 0)}/s`}
            tick={{ fontSize: 10, fill: "#71717a" }}
            axisLine={false}
            tickLine={false}
            width={70}
          />
          <Tooltip
            content={({ active, payload }) => {
              if (active && payload && payload.length) {
                const down = (payload[0]?.value as number) || 0;
                const up = (payload[1]?.value as number) || 0;
                return (
                  <div className="bg-popover/90 backdrop-blur border border-border px-3 py-2 rounded-lg shadow-md text-xs space-y-1">
                    <div className="flex items-center gap-2 text-sky-400">
                      <span className="w-2 h-2 rounded-full bg-sky-400" />
                      <span>Down: {formatBytes(down, 1)}/s</span>
                    </div>
                    <div className="flex items-center gap-2 text-emerald-400">
                      <span className="w-2 h-2 rounded-full bg-emerald-400" />
                      <span>Up: {formatBytes(up, 1)}/s</span>
                    </div>
                  </div>
                );
              }
              return null;
            }}
          />
          <Area
            type="monotone"
            dataKey="down"
            stroke="#38bdf8"
            strokeWidth={2}
            fillOpacity={1}
            fill="url(#downGradient)"
            isAnimationActive={false}
          />
          <Area
            type="monotone"
            dataKey="up"
            stroke="#34d399"
            strokeWidth={2}
            fillOpacity={1}
            fill="url(#upGradient)"
            isAnimationActive={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
};
