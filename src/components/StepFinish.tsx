import type { BootCheckRequest, OsFamily, StageReport } from "../types";
import { DriversBlock } from "./StepDrivers";
import { VerifyBlock } from "./StepVerify";
import { Fold, Note } from "./ui";

/**
 * Bước cuối: mọi việc *sau khi* đã ghi xong.
 *
 * Kiểm tra khởi động và kèm driver từng là hai bước riêng nối đuôi nhau. Cả hai
 * đều không phải thứ người dùng chọn để đi tới — chúng chỉ có nghĩa khi chiếc
 * USB đã nằm đó rồi. Gộp lại thành một trang: kết luận kiểm tra hiện ngay vì nó
 * trả lời câu hỏi duy nhất còn lại ("ghi xong rồi, có dùng được không?"), còn
 * phần driver gập lại vì phần lớn người dùng không cần tới.
 */
export function StepFinish({
  family,
  request,
  writeDone,
  isoDiscarded,
  driveLetter,
  admin,
  onAdminRelaunch,
  onStaged,
  staged,
}: {
  family: OsFamily;
  request: BootCheckRequest | null;
  writeDone: boolean;
  isoDiscarded: boolean;
  driveLetter: string | null;
  admin: boolean;
  onAdminRelaunch: () => void;
  onStaged: (r: StageReport | null) => void;
  staged: StageReport | null;
}) {
  if (!writeDone) {
    return (
      <>
        <div className="main__head"><h1>Xong</h1></div>
        <Note type="warn" icon="!">
          Hãy hoàn tất bước ghi trước. Bước này đọc lại chính chiếc USB vừa ghi.
        </Note>
      </>
    );
  }

  return (
    <>
      <div className="main__head">
        <h1>USB đã sẵn sàng</h1>
        <p>Cắm vào máy cần cài, vào menu boot (thường là F12, F9 hoặc Esc) rồi chọn thiết bị USB.</p>
      </div>

      <VerifyBlock request={request} writeDone={writeDone} isoDiscarded={isoDiscarded} />

      {/* Driver chỉ có nghĩa với Windows: nhân Linux đã mang sẵn driver trong
          chính nó, không có chỗ nào để nhồi thêm vào. */}
      {family === "windows" && (
        <Fold
          title="Kèm driver vào USB"
          hint={staged ? `Đã kèm ${staged.packages} gói` : "Tuỳ chọn — để cài xong là có Wi-Fi ngay"}
          open={!!staged}
        >
          <DriversBlock
            driveLetter={driveLetter}
            admin={admin}
            onAdminRelaunch={onAdminRelaunch}
            onStaged={onStaged}
          />
        </Fold>
      )}
    </>
  );
}
