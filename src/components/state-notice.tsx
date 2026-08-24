import type { ReasonCode } from "../../contracts/ui/state-contract";
import { reasonCopy } from "../lib/copy";
import { useI18n } from "../lib/i18n";
import { AppIcon } from "./app-icon";

const vietnameseReasonCopy: Record<ReasonCode, { title: string; detail: string }> = {
  "inventory.empty": { title: "Chưa có dữ liệu", detail: "Hãy làm mới để kiểm tra máy này." },
  "inventory.loading": { title: "Đang kiểm tra", detail: "STM đang đọc trạng thái công cụ." },
  "inventory.partial": { title: "Dữ liệu chưa đầy đủ", detail: "Một số nguồn chưa phản hồi. Chỉ thao tác với các mục đã xác minh." },
  "inventory.stale": { title: "Dữ liệu cần làm mới", detail: "Hãy quét lại trước khi cài đặt hoặc cập nhật." },
  "mapping.unsupported": { title: "Chưa hỗ trợ", detail: "STM chưa có cách cài đặt an toàn cho mục này." },
  "mapping.blocked": { title: "Đang bị chặn", detail: "Mục này chưa đáp ứng điều kiện an toàn." },
  "manager.unavailable": { title: "Thiếu nguồn cài đặt", detail: "STM sẽ đề xuất cài nguồn phù hợp khi có thể." },
  "network.offline": { title: "Không có mạng", detail: "Không thể kiểm tra phiên bản mới lúc này." },
  "operation.cancelled": { title: "Đã hủy", detail: "Không có thay đổi mới nào được tiếp tục." },
  "operation.failed": { title: "Thao tác thất bại", detail: "Xem thông báo ngắn bên dưới và thử lại." },
  "operation.recovery_available": { title: "Có thể khôi phục", detail: "STM đã chuẩn bị bước khôi phục an toàn." },
  "skill.local_modification": { title: "Có thay đổi cục bộ", detail: "Cần xem lại trước khi thay thế kỹ năng." },
  "skill.partial_failure": { title: "Kỹ năng chưa hoàn tất", detail: "Một số đích cài đặt cần xử lý lại." },
  "product_update.recovery_available": { title: "Có thể khôi phục ứng dụng", detail: "Bản cập nhật STM cần bước khôi phục." },
  "source.invalid": { title: "Nguồn không hợp lệ", detail: "Hãy kiểm tra lại liên kết." },
  "source.review_required": { title: "Cần xem lại nguồn", detail: "STM chưa thể tự động tin cậy nguồn này." },
  "mcp.auth_reference_missing": { title: "Thiếu thông tin xác thực MCP", detail: "Hãy cấu hình tham chiếu thông tin xác thực." },
  "mcp.client_unsupported": { title: "Ứng dụng MCP chưa được hỗ trợ", detail: "Liên kết này chỉ có thể xem." },
  "mcp.health_degraded": { title: "Kết nối MCP không ổn định", detail: "Hãy kiểm tra máy chủ trước khi sử dụng." },
};

export function StateNotice({ reasonCode }: { reasonCode?: ReasonCode }) {
  const { locale } = useI18n();
  if (!reasonCode) return null;
  const copy = locale === "vi" ? vietnameseReasonCopy[reasonCode] : reasonCopy[reasonCode];
  const isFailure = reasonCode.includes("failed") || reasonCode.includes("blocked");
  return (
    <section className={`state-notice ${isFailure ? "state-danger" : ""}`} aria-live="polite">
      <AppIcon name={isFailure ? "failure" : "info"} />
      <div><strong>{copy.title}</strong><p>{copy.detail}</p></div>
    </section>
  );
}
