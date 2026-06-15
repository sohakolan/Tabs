cask "tabs" do
  version "0.2.1"
  sha256 "75d79eee87e0bf2303adfeb7e7241776794f31cabe8f53a6ef29ae67a938332c"

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
