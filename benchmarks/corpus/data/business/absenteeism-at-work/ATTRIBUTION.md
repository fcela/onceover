# Absenteeism at Work — attribution

- **Source**: UCI Machine Learning Repository, *Absenteeism at Work*
  (dataset 445),
  https://archive.ics.uci.edu/static/public/445/data.csv
- **Creators**: Andrea Martiniano, Ricardo Ferreira.
- **License**: CC BY 4.0 (per the dataset page).
- **Citation requested by the source**: DOI 10.24432/C5X882.
  Dataset page: https://archive.ics.uci.edu/dataset/445/absenteeism+at+work
- **Retrieved**: 2026-11-01. Records of absenteeism at work from
  July 2007 to July 2010 at a courier company in Brazil (dataset
  abstract). 740 records, 21 attributes; the source reports no
  missing values.

## Preprocessing log (format-standard preparation)

The served file uses short column names and numeric codes for every
categorical variable. This preparation renames every column to a
fully descriptive name and maps each documented code to its natural
label, using the code lists on the dataset page:

1. Reason for absence 1–21 -> the ICD category names as listed on
   the page; 22–28 -> patient follow-up, medical consultation,
   blood donation, laboratory examination, unjustified absence,
   physiotherapy, dental consultation.
2. Day of the week 2–6 -> monday–friday. Season 1–4 -> summer,
   autumn, winter, spring. Education 1–4 -> high school, graduate,
   postgraduate, master and doctor. Month 1–12 -> month names.
3. Disciplinary failure, social drinker, social smoker: 0 -> no,
   1 -> yes.
4. Codes outside the documented lists (0 in reason for absence and
   in month of absence) are labelled "not recorded (code 0 in
   source)"; no rows are dropped and no numeric value is altered.
5. `Son` -> `number_of_children` and `Pet` -> `number_of_pets`
   (the page glosses both as counts); `ID` -> `employee_id`.
