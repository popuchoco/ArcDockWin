<div align="center">

# ArcDockWin

[繁體中文說明](README.zh-TW.md)

**Dock e barra de menus no estilo do macOS para o Windows 11, em Rust.**

Leve (um único `.exe`, sem runtime), nativo (Win32 + Direct2D + Composition) e feito para
substituir a barra de tarefas do Windows no dia a dia.

![ArcDockWin](docs/screenshots/desktop.png)

</div>

---

## ✨ Recursos

### Dock
- Vidro desfocado de verdade (o que está atrás aparece borrado), cantos arredondados e sombra suave.
- Ampliação dos ícones ao passar o mouse e pontinho embaixo dos apps abertos.
- Ampliação com movimento suavizado para evitar oscilações; a roda do mouse sobre um app com várias janelas alterna entre elas.
- **Ícone pulando** quando um app pede atenção, com um pontinho vermelho até você abrir o app.
- **Arrastar para reordenar**: solte um app aberto entre os fixados para fixar, ou arraste para cima para desafixar.
  O lançador fica sempre como último app.
- **Lixeira e Downloads** no fim da dock, com painel de arquivos recentes.
- Esconde sozinha quando uma janela encosta nela.
- Segue o **Modo Escuro/Claro** do Windows.

### Barra do topo
- Aparece em **todos os monitores** (a dock fica só no principal).
- **Assume a cor do app em foco** (10% mais escura, para separar a barra do app), na hora em que você troca de janela.
- CPU e RAM à esquerda; relógio no centro, com calendário ao clicar.
- Ícones da bandeja à direita, com painel dos ícones ocultos e escolha de quais ficam na barra.

### Central de Controle
No visual do macOS Tahoe, com cartões arredondados:
- Wi‑Fi / Rede, Bluetooth, Modo Escuro e Energia (bateria, desligar, reiniciar…).
- **Tocando agora** (capa, título e botões de qualquer player que fale com o Windows: Spotify, navegador…).
- **Brilho** por DDC/CI do monitor onde está a barra que você clicou, e **volume**.

### Segurança
- A barra de tarefas do Windows fica escondida enquanto o ArcDockWin roda, inclusive quando o Windows tenta
  mostrá-la de novo (ao tirar print, por exemplo).
- Um processo **vigia** restaura a barra do Windows se o ArcDockWin travar ou for encerrado à força.

## 📸 Prints

| Central de Controle | Dock |
| :---: | :---: |
| ![Central de Controle](docs/screenshots/central.png) | ![Dock](docs/screenshots/dock.png) |

| Barra do topo | Lançador |
| :---: | :---: |
| ![Barra do topo](docs/screenshots/topbar.png) | ![Lançador](docs/screenshots/launcher.png) |

## 🚀 Instalação

### Baixar pronto

1. Baixe o `arcdockwin.exe` da [última versão](https://github.com/claudsondouglas/ArcDockWin/releases/latest)
   (ou o `.zip`, que já vem com os scripts de ícones).
2. Coloque numa pasta fixa (por exemplo `C:\Programas\ArcDockWin`) e abra.

> O `.exe` não é assinado digitalmente, então o Windows pode mostrar **"O Windows protegeu o computador"**.
> Clique em **Mais informações → Executar assim mesmo**.

Requisito: **Windows 11**.

### Compilar do código

Requisitos: **Windows 11** e o [Rust](https://rustup.rs) (toolchain MSVC).

```powershell
git clone https://github.com/claudsondouglas/ArcDockWin.git
cd ArcDockWin
cargo build --release
.\target\release\arcdockwin.exe
```

Na primeira execução o ArcDockWin se registra para **iniciar com o Windows** (em `HKCU\...\Run`), apontando para o
`.exe` no lugar onde ele está: por isso, não deixe ele na pasta Downloads.

### Como sair
Clique com o botão direito num espaço vazio da dock → **Sair do ArcDockWin**. A barra de tarefas do Windows volta
na hora.

> Se algo der errado e a barra do Windows não voltar, abra o ArcDockWin de novo e saia pelo menu, ou reinicie o
> Explorer (Gerenciador de Tarefas → Windows Explorer → Reiniciar).

### Configuração
Tudo fica em `%APPDATA%\ArcDockWin\`:

| Arquivo | O que guarda |
| --- | --- |
| `pinned.txt` | Apps fixados na dock, um caminho por linha (pode editar à mão). |
| `icons.txt` | Ícones trocados por app (`nome.exe=C:\caminho\icone.ico`), veja abaixo. |
| `colors.db` | Cor de cada app na barra do topo e quais ícones da bandeja ficam visíveis (SQLite). |

## 🎨 Ícones do macOS (MacTahoe)

A dock usa o ícone do próprio app por padrão. Para deixar tudo com cara de Mac, a pasta [`icons/`](icons) tem dois
scripts que aplicam o tema [**MacTahoe**](https://github.com/vinceliuice/MacTahoe-icon-theme) de
[vinceliuice](https://github.com/vinceliuice) na dock **e no Windows inteiro**: atalhos da área de trabalho e do menu
Iniciar, pastas comuns e conhecidas do Explorer (Área de Trabalho, Documentos, Downloads, Imagens, Música e Vídeos),
discos, Lixeira e Este Computador. O script usa os pontos de personalização do Windows Shell e salva um backup para
restaurar as associações originais.

![Ícones MacTahoe](docs/screenshots/icones.png)

### 1. Instale o ImageMagick

```powershell
winget install ImageMagick.ImageMagick
```

Depois **feche e abra o terminal** para o comando `magick` passar a existir.

### 2. Baixe e converta os ícones

```powershell
powershell -ExecutionPolicy Bypass -File .\icons\Converter.ps1
```

O script baixa o tema, converte os ~1400 ícones de SVG para `.ico` (leva cerca de 1 minuto) e monta tudo em
`C:\Icones\MacTahoe`:

```
C:\Icones\MacTahoe\
├── apps\          ícones de apps (google-chrome.ico, spotify.ico, …)
├── sistema\       pastas, Lixeira, discos e Este Computador
├── aliases.txt    nomes alternativos de cada ícone
└── IconesTahoe.ps1
```

> Use essa pasta: os ícones da Lixeira e de Downloads da dock são lidos de `C:\Icones\MacTahoe\sistema`.

### 3. Aplique

```powershell
cd C:\Icones\MacTahoe
powershell -ExecutionPolicy Bypass -File .\IconesTahoe.ps1 -Testar     # só mostra qual ícone cada atalho vai ganhar
powershell -ExecutionPolicy Bypass -File .\IconesTahoe.ps1 -Instalar   # aplica e fica na bandeja
```

O `-Instalar` pede permissão de administrador (pastas e discos são configurações do sistema) e cria uma tarefa
agendada que abre o **Ícones Tahoe** na bandeja a cada logon. Ele reaplica os ícones de tempos em tempos, para pegar
apps novos e atalhos que as atualizações recriam. Pelo menu da bandeja dá para:

- **Aplicar agora** (ou dar dois cliques no ícone);
- escolher o intervalo em **Aplicar automaticamente** (1 h a 24 h, ou desligado);
- **Restaurar ícones do Windows**.

A dock recebe os ícones pelo arquivo `%APPDATA%\ArcDockWin\icons.txt`, que o script escreve. O botão Iniciar ganha
o ícone de grade de apps.

### Ícone errado ou faltando?

O tema é feito para Linux, então o script procura o ícone pelo nome do `.exe`, pelo nome do atalho e pelos aliases do
tema. Quando o nome do programa no Windows não bate, adicione uma linha na tabela `$Extra` no começo do
`IconesTahoe.ps1`:

```powershell
$Extra = @{
  msedge = 'microsoft-edge'   # nome do .exe (sem .exe) = nome do ícone em apps\ (sem .ico)
  code   = 'visual-studio-code'
  # ...
}
```

Para trocar o ícone de um único app na dock, sem script, edite `%APPDATA%\ArcDockWin\icons.txt`:

```
spotify.exe=C:\Icones\MacTahoe\apps\spotify.ico
```

### Desfazer

```powershell
powershell -ExecutionPolicy Bypass -File C:\Icones\MacTahoe\IconesTahoe.ps1 -Desinstalar
```

Isso volta todos os ícones originais (o script guarda um backup do que trocou) e remove a tarefa agendada.

## 🛠️ Como funciona

| Parte | Tecnologia |
| --- | --- |
| Desenho | [tiny-skia](https://github.com/RazrFalcon/tiny-skia) + Direct2D/DirectWrite, janelas em camadas (`UpdateLayeredWindow`) |
| Vidro | `Windows.UI.Composition` (backdrop do DWM) |
| Reserva de espaço | AppBar (`SHAppBarMessage`) |
| Janelas e foco | `SetWinEventHook` e `RegisterShellHookWindow` |
| Mídia | `Windows.Media.Control` (GSMTC) |
| Brilho | DDC/CI (`dxva2`) |
| Cores e bandeja | SQLite do próprio Windows (`winsqlite3.dll`) |

## 🙏 Créditos

- Ícones: [MacTahoe icon theme](https://github.com/vinceliuice/MacTahoe-icon-theme) por vinceliuice (GPL-3.0). Eles não
  vêm neste repositório: o `Converter.ps1` baixa direto do projeto original.
- Ideia da rede de segurança da barra de tarefas: [Lemxy/WinRust](https://github.com/Lemxy/WinRust).
- Inspirado no macOS. Não é afiliado à Apple.

## 📄 Licença

[MIT](LICENSE). Os ícones MacTahoe, baixados à parte pelo `Converter.ps1`, seguem a licença GPL-3.0 do projeto original.
