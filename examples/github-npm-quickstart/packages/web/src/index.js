import { formatReleasePlan } from "@acme/shared";

export function renderDashboardVersion(version) {
	return formatReleasePlan(`dashboard ${version}`);
}
