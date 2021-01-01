function renderInteger(number) {
  var fmt = Intl.NumberFormat('en-EN').format(number);
  var parts = fmt.split(',');
  var str = '';
  for (var i = 0; i < parts.length - 1; ++i) {
    str += '<span class="digit-sep">' + parts[i] + '</span>';
  }
  str += '<span>' + parts[parts.length - 1] + '</span>';
  return str;
}

webix.ready(function(){
  var tzOffset = new Date().getTimezoneOffset();
  var tzString;
  if (tzOffset < 0) {
    tzString = moment.utc(moment.duration(-tzOffset, 'minutes').asMilliseconds()).format('+HH:mm');
  } else {
    tzString = moment.utc(moment.duration(tzOffset, 'minutes').asMilliseconds()).format('-HH:mm');
  }
  webix.ui({
    container: "blocks-table",
    view: "datatable",
    columns:[
      {
        width: 160,
        id: "age",
        header: "Age",
        template: function (row) {
          return '<a href="/block/' + row.hash + '">' + moment(row.timestamp).fromNow() + '</a>';
        },
      },
      {
        width: 200,
        id: "timestamp",
        header: "Date (UTC" + tzString + ")",
        template: function (row) {
          return moment(row.timestamp).format('ll, LTS');
        },
      },
      {
        width: 80,
        id: "height",
        header: "Height",
        template: function (row) {
          return renderInteger(row.height);
        },
      },
      {
        width: 500,
        id: "hash",
        header: "Block Hash",
        css: "hash",
      },
      {
        width: 100,
        id: "size",
        header: "Size",
        css: "size",
        template: function (row) {
          if (row.size < 1024) {
            return row.size + ' B';
          } else if (row.size < 1024 * 1024) {
            return (row.size / 1000).toFixed(2) + ' kB';
          } else {
            return (row.size / 1000000).toFixed(2) + ' MB';
          }
        },
      },
      {
        width: 120,
        id: "numTxs",
        header: "Transactions",
        template: function (row) {
          return renderInteger(row.numTxs);
        }
      },
      {
        width: 130,
        id: "difficulty",
        header: "Est. Hashrate",
        template: function (row) {
          var estHashrate = row.difficulty * 0xffffffff / 600;
          if (estHashrate < 1e12) {
            return (estHashrate / 1e9).toFixed(2) + ' GH/s';
          } else if (estHashrate < 1e15) {
            return (estHashrate / 1e12).toFixed(2) + ' TH/s';
          } else if (estHashrate < 1e18) {
            return (estHashrate / 1e15).toFixed(2) + ' PH/s';
          } else {
            return (estHashrate / 1e18).toFixed(2) + ' EH/s';
          }
        },
      },
    ],
    autowidth: true,
    //autoheight: true,
    data: blockData,
  });	
});
