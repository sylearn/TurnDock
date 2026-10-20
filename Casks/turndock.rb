# 由 scripts/cask.sh 生成，不要手改。
cask "turndock" do
  version "0.1.0"
  sha256 "d6480bd94636b9be6b91af2edbdcea6f312894e30ca2137e73e42ee8b7beb25a"

  url "https://github.com/sylearn/TurnDock/releases/download/v#{version}/TurnDock.app.tar.gz"
  name "TurnDock"
  name "回合"
  desc "你和 AI 之间的交接点"
  homepage "https://github.com/sylearn/TurnDock"

  auto_updates true
  depends_on macos: ">= :ventura"

  app "TurnDock.app"
  # 终端输入 turndock 打开窗口；AI 客户端用 `turndock mcp` 连接。
  binary "#{appdir}/TurnDock.app/Contents/MacOS/turndock"

  # 尚未做 Apple 公证：去掉隔离标记，避免首次打开时被拦截。
  postflight do
    system_command "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "#{appdir}/TurnDock.app"]
  end

  zap trash: "~/Library/Application Support/TurnDock"
end
