# Abacus

Suíte de escritório de código aberto, começando pela planilha. O aplicativo
desktop usa [Tauri](https://tauri.app) e o motor de cálculo é o
[IronCalc](https://www.ironcalc.com), ambos em Rust.

O Abacus usa um [fork do IronCalc](https://github.com/tamadeu/IronCalc/tree/portugues-brasil)
que acrescenta o português do Brasil: nomes de função (`SOMA`, `SE`,
`PROCV`), erros (`#NOME?`), números e datas no formato brasileiro e `R$`
como moeda.

![Abacus com um orçamento aberto de um arquivo .xlsx: título mesclado, tabela com bordas, texto quebrado em linhas e valores em reais](docs/screenshot.png)

## Estado atual

Projeto no início. O que já funciona:

- Abrir e salvar arquivos `.xlsx` (Ctrl+O, Ctrl+S, Ctrl+Shift+S), com aviso
  antes de descartar alterações não salvas.
- Várias planilhas por arquivo: criar, renomear (dois cliques na aba) e
  excluir, com fórmulas entre planilhas.
- Grade desenhada em canvas, com rolagem e seleção de uma célula ou de um
  intervalo, por mouse ou teclado (Shift+setas). Clicar num cabeçalho
  seleciona a coluna ou a linha inteira, e arrastar além da borda rola a
  planilha.
- Formatação pela barra de ferramentas: negrito, itálico, sublinhado,
  tachado, tamanho da fonte, cor do texto, cor de fundo, alinhamento
  horizontal e vertical e formato numérico
  (geral, duas casas, moeda, porcentagem), com Ctrl+B, Ctrl+I e Ctrl+U.
- Larguras de coluna e alturas de linha: vêm do arquivo, mudam arrastando a
  borda do cabeçalho, e dois cliques na borda ajustam a coluna ao conteúdo.
- Células mescladas, bordas e quebra de texto: aparecem como no arquivo e
  podem ser aplicadas pela barra de ferramentas. A linha cresce sozinha
  quando o texto quebrado não cabe.
- Inserir e excluir linhas e colunas pelo menu do botão direito, que também
  tem recortar, copiar e colar.
- Edição direto na célula (digitando, com F2 ou com dois cliques) e pela
  barra de fórmulas; Enter, Tab e as setas gravam, Esc cancela e Delete apaga
  a seleção.
- Ao escrever uma fórmula, os intervalos citados ficam coloridos na grade,
  clicar ou arrastar nas células insere a referência, e uma lista sugere as
  funções que começam com o que foi digitado (Tab aceita).
- Alça de preenchimento no canto da seleção: arrastar continua sequências
  (1, 2, 3…) e copia fórmulas ajustando as referências.
- Copiar, recortar e colar (Ctrl+C, Ctrl+X, Ctrl+V). Dentro do Abacus a
  colagem leva fórmulas, com as referências ajustadas, e estilos; a área de
  transferência do sistema recebe e aceita texto separado por tabulações,
  para trocar dados com outros programas.
- Desfazer e refazer (Ctrl+Z, Ctrl+Y).
- As fórmulas e funções do IronCalc, com os nomes do Excel em português,
  incluindo `LET`, `LAMBDA` e matrizes dinâmicas. Arquivos `.xlsx` continuam
  compatíveis: no arquivo as funções ficam gravadas em inglês, como no Excel.
- Números, datas e moeda no formato brasileiro (`18.500,00`, `14/11/2026`,
  `R$ 10,50`).

Limitações conhecidas:

- Os nomes em português de algumas funções muito recentes do Excel
  (`MAPA`, `REGEX.TESTAR`, `PORCENTAGEMDE`, `APARARINTERVALO`) vieram da
  documentação da Microsoft e não foram conferidos num Excel em português.
- A coluna não alarga sozinha quando o número digitado não cabe; ele aparece
  como `####` até a coluna ser ajustada.
- As referências coloridas aparecem na grade, mas não no texto da fórmula.
- A lista de funções sugere só os nomes, sem descrição nem argumentos.
- Clicar para inserir referência só funciona dentro da mesma planilha.
- Colar para outros programas leva só os valores, sem formatação.
- Tipo de fonte e gráficos do arquivo ainda não aparecem na grade.
- As bordas aplicadas pela barra são sempre finas e pretas; as do arquivo
  aparecem com cor e espessura, mas a dupla é desenhada como grossa.
- Bordas e mesclagem não funcionam numa linha ou coluna inteira.
- Fechar a janela não avisa sobre alterações não salvas.
- Arquivos gerados pela biblioteca openpyxl mostram `0` em células vazias que
  só têm estilo. É um defeito do importador do IronCalc com células gravadas
  como `t="n"` sem valor; arquivos no formato que o Excel grava não têm o
  problema.

## Estrutura

| Pasta | Conteúdo |
| --- | --- |
| `apps/desktop/src` | Interface em TypeScript (Vite). |
| `apps/desktop/src-tauri` | Casca Tauri. `engine.rs` é a ponte com o IronCalc. |

A interface fala com o motor apenas pela interface `Engine`
(`apps/desktop/src/engine.ts`). Hoje ela chama comandos do Tauri; a versão web
vai implementar a mesma interface com o IronCalc compilado para WebAssembly.

## Como rodar

Pré-requisitos: [Rust](https://rustup.rs), Node.js 20 ou mais novo e as
[dependências de sistema do Tauri](https://tauri.app/start/prerequisites/).

```sh
cd apps/desktop
npm install
npm run tauri dev
```

`npm run dev` abre só a interface no navegador, sem o motor de cálculo, o que
serve para trabalhar no visual.

Testes, a partir da raiz:

```sh
cargo test
```

## Próximos passos

1. Localizar e substituir.
2. Congelar painéis e ocultar linhas e colunas.
3. Classificar e filtrar.
4. Propor o português do Brasil ao IronCalc original.
5. Versão web com o motor em WebAssembly.

## Licença

[MIT](LICENSE). O IronCalc e o Tauri, de que o Abacus depende, são
distribuídos sob MIT ou Apache-2.0.
