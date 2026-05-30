import type { ApiKey } from "@/types";

export const ALL_API_KEY_GROUP_VALUE = "__all__";
export const UNGROUPED_API_KEY_GROUP_VALUE = "__ungrouped__";

export type ApiKeyGroupOption = {
  label: string;
  value: string;
  count: number;
};

function normalizeGroupName(value: string | null | undefined): string {
  return String(value || "").trim();
}

export function buildApiKeyGroupOptions(apiKeys: ApiKey[]): ApiKeyGroupOption[] {
  const counts = new Map<string, number>();
  for (const key of apiKeys) {
    const groupName = normalizeGroupName(key.groupName);
    const groupValue = groupName || UNGROUPED_API_KEY_GROUP_VALUE;
    counts.set(groupValue, (counts.get(groupValue) || 0) + 1);
  }

  const namedGroups = Array.from(counts.entries())
    .filter(([value]) => value !== UNGROUPED_API_KEY_GROUP_VALUE)
    .sort(([left], [right]) => left.localeCompare(right, "zh-CN"))
    .map(([value, count]) => ({
      value,
      label: value,
      count,
    }));

  const options: ApiKeyGroupOption[] = [
    {
      value: ALL_API_KEY_GROUP_VALUE,
      label: "全部",
      count: apiKeys.length,
    },
    ...namedGroups,
  ];

  const ungroupedCount = counts.get(UNGROUPED_API_KEY_GROUP_VALUE) || 0;
  if (ungroupedCount > 0) {
    options.push({
      value: UNGROUPED_API_KEY_GROUP_VALUE,
      label: "未分组",
      count: ungroupedCount,
    });
  }

  return options;
}

export function filterApiKeysByGroup(apiKeys: ApiKey[], groupValue: string): ApiKey[] {
  if (groupValue === ALL_API_KEY_GROUP_VALUE) {
    return apiKeys;
  }
  if (groupValue === UNGROUPED_API_KEY_GROUP_VALUE) {
    return apiKeys.filter((key) => !normalizeGroupName(key.groupName));
  }
  return apiKeys.filter((key) => normalizeGroupName(key.groupName) === groupValue);
}
