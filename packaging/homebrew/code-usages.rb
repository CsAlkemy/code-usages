# Homebrew cask — lives in YOUR tap repo, not here. Setup (one time):
#
#   1. Create a GitHub repo named  homebrew-tap  under your account.
#   2. Copy this file to  Casks/code-usages.rb  in that repo.
#   3. After each release, update `version` and the two `sha256` values:
#        shasum -a 256 code-usages-<version>-arm64.dmg code-usages-<version>-x64.dmg
#
# Users then install with:
#   brew install CsAlkemy/tap/code-usages
cask "code-usages" do
  version "0.3.0"

  on_arm do
    sha256 "TODO_SHA256_OF_ARM64_DMG"
    url "https://github.com/csalkemy/code-usages/releases/download/v#{version}/code-usages-#{version}-arm64.dmg"
  end
  on_intel do
    sha256 "TODO_SHA256_OF_X64_DMG"
    url "https://github.com/csalkemy/code-usages/releases/download/v#{version}/code-usages-#{version}-x64.dmg"
  end

  name "Code Usages"
  desc "Menu-bar ring showing your claude.ai plan usage"
  homepage "https://github.com/csalkemy/code-usages"

  auto_updates true

  app "Code Usages.app"

  zap trash: [
    "~/Library/Application Support/Code Usages",
    "~/Library/Preferences/com.tuliptech.code-usages.plist",
  ]
end
