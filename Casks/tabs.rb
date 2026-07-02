cask "tabs" do
  version "0.2.8"
  sha256 "0d041e8b0faa5004e06a4d98251ade00d2e0c6129b9a890c7a29bce63ab262d1"

  url "https://github.com/sohakolan/Tabs/releases/download/v#{version}/Tabs-arm64.dmg"
  name "Tabs"
  desc "Lightweight, fast window switcher"
  homepage "https://github.com/sohakolan/Tabs"

  depends_on macos: :sonoma
  depends_on arch: :arm64

  app "Tabs.app"

  caveats <<~EOS
    Tabs is not notarized by Apple, so macOS blocks it on first launch.
    Clear the quarantine flag, then open it:
      xattr -dr com.apple.quarantine "/Applications/Tabs.app"
  EOS
end
