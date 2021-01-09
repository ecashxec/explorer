webix.ready(function(){
  webix.ui({
    container: "txs-table",
    view: "datatable",
    columns:[
      {
        width: 450,
        id: "txHash",
        header: "ID",
        css: "hash",
        template: function (row) {
          return '<a href="/tx/' + row.txHash + '">' + row.txHash + '</a>';
        },
      },
      {
        width: 70,
        id: "size",
        header: "Size",
        template: function (row) {
          return formatByteSize(row.size);
        },
      },
      {
        width: 55,
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
        width: 100,
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
        id: "satsOutput",
        header: "Output Amount",
        adjust: true,
        template: function(row) {
          if (row.token !== null) {
            var ticker = ' <a href="/tx/' + row.token.tokenId + '">' + row.token.tokenTicker + '</a>';
            return renderAmount(row.tokenOutput, row.token.decimals) + ticker;
          }
          return renderSats(row.satsOutput) + ' ABC';
        },
      },
    ],
    //autoheight: true,
    data: txData,
  });	
});
