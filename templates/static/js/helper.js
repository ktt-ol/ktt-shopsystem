jQuery["postJSON"] = function( url, data, callback ) {
	// shift arguments if data argument was omitted
	if ( jQuery.isFunction( data ) ) {
		callback = data;
		data = undefined;
	}

	return jQuery.ajax({
		url: url,
		type: "POST",
		contentType: "application/json; charset=utf-8",
		dataType: "json",
		data: JSON.stringify(data),
		success: callback
	});
};

var toggle_button = function(buttonid, text1, text2, buttonstate) {
	if (buttonstate) {
		$(buttonid).html(text1)
		$(buttonid).addClass("btn-success")
		$(buttonid).removeClass("btn-danger")
	} else {
		$(buttonid).html(text2)
		$(buttonid).addClass("btn-danger")
		$(buttonid).removeClass("btn-success")
	}
}

var toggle_button_yes_no = function(buttonid, buttonstate) {
	return toggle_button(buttonid, "Yes", "No", buttonstate);
}

var toggle_button_active_inactive = function(buttonid, buttonstate) {
	return toggle_button(buttonid, "Active", "Inactive", buttonstate);
}

var cent2euro = function(cent) {
	euro = Math.floor(cent / 100).toString();
	cent = Math.floor(cent % 100).toString().padStart(2, '0');
	return euro + "." + cent;
}

var euro2cent = function(euro) {
	euro = euro.replace(',', '.');
	parts = euro.split('.', 2);
	if (parts.length == 2) {
		if (parts[1].length > 2)
			return NaN;
		euro = parseInt(parts[0], 10);
		cent = parseInt(parts[1], 10);
	} else {
		euro = 0
		cent = parseInt(parts[0], 10);
	}
	return euro * 100 + cent;
}

var ts2isotime = function(timestamp) {
	dt = new Date(timestamp * 1000);
	valid_since = dt.toISOString();
	valid_since_date = valid_since.slice(0, 10);
	valid_since_time = valid_since.slice(11, 16);
	return valid_since_date+' '+valid_since_time;
}

var isodate2ts = function(iso) {
	return Math.floor(new Date(iso+"T23:59:59").getTime() / 1000);
}
