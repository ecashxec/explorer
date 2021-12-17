var isSatsTableLoaded = false;
function loadSatsTable() {
  if (!isSatsTableLoaded) {
    console.log(addrBalances[0].utxos);
    webix.ui({
      container: "sats-coins-table",
      view: "datatable",
      columns:[
        {
          id: "outpoint",
          header: "Outpoint",
          css: "hash",
          adjust: true,
          template: function (row) {
            return '<a href="/tx/' + row.txHash + '">' + 
              row.txHash + ':' + row.outIdx +
              (row.isCoinbase ? '<div class="ui green horizontal label">Coinbase</div>' : '') +
              '</a>';
          },
        },
        {
          id: "blockHeight",
          header: "Block Height",
          adjust: true,
          template: function (row) {
            return '<a href="/block-height/' + row.blockHeight + '">' + renderInteger(row.blockHeight) + '</a>';
          },
        },
        {
          id: "amount",
          header: "XEC amount",
          adjust: true,
          template: function (row) {
            return renderSats(row.satsAmount) + ' XEC';
          },
        },
      ],
      autoheight: true,
      autowidth: true,
      data: addrBalances[0].utxos,
    });
    isSatsTableLoaded = true;
  }
}

var isTokenTableLoaded = {};
function loadTokenTable(balanceIdx) {
  if (!isTokenTableLoaded[balanceIdx]) {
    console.log(addrBalances[balanceIdx].utxos);
    webix.ui({
      container: "tokens-coins-table-" + balanceIdx,
      view: "datatable",
      columns:[
        {
          id: "outpoint",
          header: "Outpoint",
          css: "hash",
          adjust: true,
          template: function (row) {
            return '<a href="/tx/' + row.txHash + '">' + 
              row.txHash + ':' + row.outIdx +
              (row.isCoinbase ? '<div class="ui green horizontal label">Coinbase</div>' : '') +
              '</a>';
          },
        },
        {
          id: "blockHeight",
          header: "Block Height",
          adjust: true,
          template: function (row) {
            return '<a href="/block-height/' + row.blockHeight + '">' + renderInteger(row.blockHeight) + '</a>';
          },
        },
        {
          id: "tokenAmount",
          header: addrBalances[balanceIdx].token.tokenTicker + " amount",
          adjust: true,
          template: function (row) {
            return renderAmount(row.tokenAmount, addrBalances[balanceIdx].token.decimals) + ' ' + addrBalances[balanceIdx].token.tokenTicker;
          },
        },
        {
          id: "satsAmount",
          header: "XEC amount",
          adjust: true,
          template: function (row) {
            return renderSats(row.satsAmount) + ' XEC';
          },
        },
      ],
      autoheight: true,
      autowidth: true,
      data: addrBalances[balanceIdx].utxos,
    });
    isTokenTableLoaded[balanceIdx] = true;
  }
}

webix.ready(function(){
  webix.ui({
    container: "txs-table",
    view: "datatable",
    columns:[
      {
        id: "age",
        header: "Age",
        adjust: true,
        template: function (row) {
          if (row.timestamp == 0) {
            return '<div class="ui gray horizontal label">Mempool</div>';
          }
          return moment(row.timestamp).fromNow();
        },
      },
      {
        id: "timestamp",
        header: "Date (UTC" + tzOffset + ")",
        width: 160,
        template: function (row) {
          if (row.timestamp == 0) {
            return '<div class="ui gray horizontal label">Mempool</div>';
          }
          return moment(row.timestamp).format('ll, LTS');
        },
      },
      {
        id: "txHash",
        header: "Transaction ID",
        css: "hash",
        width: 135,
        template: function (row) {
          return '<a href="/tx/' + row.txHash + '">' + renderTxHash(row.txHash) + '</a>';
        },
      },
      {
        id: "blockHeight",
        header: "Block Height",
        width: 95,
        template: function (row) {
          if (row.timestamp == 0) {
            return '<div class="ui gray horizontal label">Mempool</div>';
          }
          return '<a href="/block-height/' + row.blockHeight + '">' + renderInteger(row.blockHeight) + '</a>';
        },
      },
      {
        id: "size",
        header: "Size",
        width: 60,
        template: function (row) {
          return formatByteSize(row.size);
        },
      },
      {
        width: 45,
        id: "fee",
        header: {text: "Fee [sats]", colspan: 2},
        css: "fee",
        template: function (row) {
          if (row.isCoinbase) {
            return '<div class="ui green horizontal label">Coinbase</div>';
          }
          const fee = row.satsInput - row.satsOutput;
          return renderInteger(fee);
        },
      },
      {
        width: 90,
        id: "feePerByte",
        css: "fee-per-byte",
        template: function (row) {
          if (row.isCoinbase) {
            return '';
          }
          const fee = row.satsInput - row.satsOutput;
          const feePerByte = fee / row.size;
          return renderInteger(Math.round(feePerByte * 1000)) + '/kB';
        },
      },
      {
        width: 55,
        id: "numInputs",
        header: "Inputs",
      },
      {
        width: 65,
        id: "numOutputs",
        header: "Outputs",
      },
      {
        id: "deltaSats",
        header: "Amount XEC",
        adjust: true,
        template: function(row) {
          return renderSats(row.deltaSats) + ' XEC';
        },
      },
      {
        id: "deltaSats",
        header: "Amount Token",
        adjust: true,
        template: function(row) {
          if (row.token !== null) {
            var ticker = ' <a href="/tx/' + row.token.tokenId + '">' + row.token.tokenTicker + '</a>';
            return renderAmount(row.deltaTokens, row.token.decimals) + ticker;
          }
          return '';
        },
      },
    ],
    data: addrTxData,
  });	
});
